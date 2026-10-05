"""Django layer for the lending library (issue #114).

A real Django project spreads this over ``models.py``, ``admin.py``,
``views.py``, ``forms.py``, ``signals.py`` and ``urls.py``; the corpus keeps
it in one file so the syntax sits together: model classes with field calls
and an inner ``Meta``, managers and querysets, ``@admin.register`` with
``ModelAdmin`` options and ``@admin.display``, class-based views with
mixins, function views under decorator stacks, a ``ModelForm``, signal
receivers and a ``urlpatterns`` table of ``path``/``re_path``/``include``.
"""

from __future__ import annotations

from datetime import timedelta
from typing import Any

from django import forms
from django.conf import settings
from django.contrib import admin, messages
from django.contrib.auth.decorators import login_required, permission_required
from django.contrib.auth.mixins import LoginRequiredMixin, PermissionRequiredMixin
from django.core.exceptions import ValidationError as DjangoValidationError
from django.db import models, transaction
from django.db.models import Count, F, Q
from django.db.models.signals import post_save, pre_delete
from django.dispatch import receiver
from django.http import HttpRequest, HttpResponse, JsonResponse
from django.shortcuts import get_object_or_404, redirect, render
from django.urls import include, path, re_path, reverse
from django.utils import timezone
from django.views import View
from django.views.decorators.cache import cache_page
from django.views.decorators.http import require_GET, require_POST
from django.views.generic import CreateView, DetailView, ListView, UpdateView

from library.errors import LimitExceededError, audit
from library.models import Genre, is_valid_isbn, normalize_isbn, overdue_fee
from library.services import build_services

LOAN_PERIOD = timedelta(days=21)


# ---------------------------------------------------------------------------
# Models
# ---------------------------------------------------------------------------


class TimeStampedModel(models.Model):
    """Abstract base: every table gets created/updated stamps."""

    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        abstract = True


class AuthorRecord(TimeStampedModel):
    name = models.CharField(max_length=200)
    born = models.DateField(null=True, blank=True)

    class Meta:
        ordering = ["name"]
        verbose_name = "author"

    def __str__(self) -> str:
        return self.name


class BookQuerySet(models.QuerySet):
    def available(self) -> BookQuerySet:
        return self.filter(copies__status=CopyRecord.Status.ON_SHELF).distinct()

    def in_genre(self, genre: Genre) -> BookQuerySet:
        return self.filter(genre=genre.name)

    def popular(self, limit: int = 10) -> BookQuerySet:
        return self.annotate(n=Count("copies__loans")).order_by("-n")[:limit]


class BookRecord(TimeStampedModel):
    GENRE_CHOICES = [(g.name, g.value) for g in Genre]

    isbn = models.CharField(max_length=13, unique=True)
    title = models.CharField(max_length=300)
    genre = models.CharField(max_length=32, choices=GENRE_CHOICES)
    authors = models.ManyToManyField(AuthorRecord, related_name="books")
    published = models.PositiveSmallIntegerField(null=True)

    objects = BookQuerySet.as_manager()

    class Meta:
        ordering = ["title"]
        indexes = [models.Index(fields=["genre", "title"])]
        constraints = [
            models.CheckConstraint(
                condition=Q(published__gte=1450) | Q(published__isnull=True),
                name="published_after_printing_press",
            ),
        ]

    def __str__(self) -> str:
        return f"{self.title} ({self.isbn})"

    def clean(self) -> None:
        self.isbn = normalize_isbn(self.isbn)
        if not is_valid_isbn(self.isbn):
            raise DjangoValidationError({"isbn": "Not a valid ISBN."})

    def get_absolute_url(self) -> str:
        return reverse("books:detail", kwargs={"isbn": self.isbn})


class CopyRecord(models.Model):
    class Status(models.TextChoices):
        ON_SHELF = "shelf", "On shelf"
        ON_LOAN = "loan", "On loan"
        LOST = "lost", "Lost"

    book = models.ForeignKey(BookRecord, on_delete=models.CASCADE, related_name="copies")
    barcode = models.CharField(max_length=32, unique=True)
    status = models.CharField(max_length=8, choices=Status.choices, default=Status.ON_SHELF)

    def __str__(self) -> str:
        return self.barcode


class LoanRecord(TimeStampedModel):
    copy = models.ForeignKey(CopyRecord, on_delete=models.PROTECT, related_name="loans")
    member = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="loans"
    )
    due = models.DateTimeField()
    returned = models.DateTimeField(null=True, blank=True)

    class Meta:
        get_latest_by = "created_at"
        permissions = [("waive_fee", "Can waive overdue fees")]

    @property
    def is_overdue(self) -> bool:
        return self.returned is None and self.due < timezone.now()

    def fee(self) -> float:
        days = (timezone.now() - self.due).days
        return overdue_fee(max(days, 0))

    def save(self, *args: Any, **kwargs: Any) -> None:
        if self.due is None:
            self.due = timezone.now() + LOAN_PERIOD
        super().save(*args, **kwargs)


# ---------------------------------------------------------------------------
# Admin
# ---------------------------------------------------------------------------


class CopyInline(admin.TabularInline):
    model = CopyRecord
    extra = 0


@admin.register(BookRecord)
class BookAdmin(admin.ModelAdmin):
    list_display = ("title", "isbn", "genre", "copy_count")
    list_filter = ("genre",)
    search_fields = ("title", "isbn", "authors__name")
    inlines = [CopyInline]
    actions = ["mark_lost"]

    @admin.display(description="Copies", ordering="n_copies")
    def copy_count(self, obj: BookRecord) -> int:
        return obj.copies.count()

    @admin.action(description="Mark every copy as lost")
    def mark_lost(self, request: HttpRequest, queryset: BookQuerySet) -> None:
        updated = CopyRecord.objects.filter(book__in=queryset).update(
            status=CopyRecord.Status.LOST
        )
        self.message_user(request, f"{updated} copies marked lost", messages.WARNING)


@admin.register(AuthorRecord, LoanRecord)
class PlainAdmin(admin.ModelAdmin):
    date_hierarchy = "created_at"


admin.site.register(CopyRecord)


# ---------------------------------------------------------------------------
# Forms
# ---------------------------------------------------------------------------


class BookForm(forms.ModelForm):
    class Meta:
        model = BookRecord
        fields = ["isbn", "title", "genre", "authors", "published"]
        widgets = {"authors": forms.CheckboxSelectMultiple}

    def clean_isbn(self) -> str:
        isbn = normalize_isbn(self.cleaned_data["isbn"])
        if not is_valid_isbn(isbn):
            raise forms.ValidationError("Not a valid ISBN.")
        return isbn


class CheckoutForm(forms.Form):
    barcode = forms.CharField(max_length=32)


# ---------------------------------------------------------------------------
# Views
# ---------------------------------------------------------------------------


class BookListView(ListView):
    model = BookRecord
    paginate_by = 25
    template_name = "books/list.html"

    def get_queryset(self) -> BookQuerySet:
        qs = BookRecord.objects.all()
        if genre := self.request.GET.get("genre"):
            qs = qs.in_genre(Genre[genre.upper()])
        return qs.prefetch_related("authors")

    def get_context_data(self, **kwargs: Any) -> dict[str, Any]:
        context = super().get_context_data(**kwargs)
        context["popular"] = BookRecord.objects.popular(5)
        return context


class BookDetailView(DetailView):
    model = BookRecord
    slug_field = "isbn"
    slug_url_kwarg = "isbn"


class BookCreateView(LoginRequiredMixin, PermissionRequiredMixin, CreateView):
    model = BookRecord
    form_class = BookForm
    permission_required = "catalogue.add_bookrecord"


class BookUpdateView(LoginRequiredMixin, UpdateView):
    model = BookRecord
    form_class = BookForm
    slug_field = "isbn"
    slug_url_kwarg = "isbn"


class StatsView(View):
    http_method_names = ["get"]

    def get(self, request: HttpRequest) -> JsonResponse:
        rows = (
            BookRecord.objects.values("genre")
            .annotate(total=Count("id"), loans=Count("copies__loans"))
            .order_by(F("loans").desc())
        )
        return JsonResponse({"genres": list(rows)})


@login_required
@require_POST
def checkout(request: HttpRequest, isbn: str) -> HttpResponse:
    book = get_object_or_404(BookRecord, isbn=normalize_isbn(isbn))
    form = CheckoutForm(request.POST)
    if not form.is_valid():
        return render(request, "books/detail.html", {"object": book, "form": form}, status=400)
    with transaction.atomic():
        copy = get_object_or_404(
            CopyRecord.objects.select_for_update(),
            book=book,
            barcode=form.cleaned_data["barcode"],
        )
        try:
            build_services(settings.DATABASES["default"]["NAME"]).checkout(
                request.user.pk, book.isbn
            )
        except LimitExceededError as exc:
            messages.error(request, str(exc))
            return redirect(book)
        LoanRecord.objects.create(copy=copy, member=request.user)
        copy.status = CopyRecord.Status.ON_LOAN
        copy.save(update_fields=["status"])
    audit("checkout", isbn=book.isbn, member=request.user.pk)
    return redirect(book)


@login_required
@permission_required("catalogue.waive_fee", raise_exception=True)
@require_POST
def waive_fee(request: HttpRequest, pk: int) -> HttpResponse:
    loan = get_object_or_404(LoanRecord, pk=pk)
    loan.returned = loan.returned or timezone.now()
    loan.save()
    messages.success(request, "Fee waived.")
    return redirect("loans:mine")


@require_GET
@cache_page(60 * 5)
def overdue(request: HttpRequest) -> JsonResponse:
    loans = LoanRecord.objects.filter(returned__isnull=True, due__lt=timezone.now())
    return JsonResponse(
        {"overdue": [{"id": loan.pk, "fee": loan.fee()} for loan in loans.select_related("copy")]}
    )


def my_loans(request: HttpRequest) -> HttpResponse:
    loans = LoanRecord.objects.filter(member=request.user).select_related("copy__book")
    return render(request, "loans/mine.html", {"loans": loans})


# ---------------------------------------------------------------------------
# Signals
# ---------------------------------------------------------------------------


@receiver(post_save, sender=LoanRecord)
def mark_copy_on_loan(sender: type[LoanRecord], instance: LoanRecord, created: bool, **kwargs: Any) -> None:
    if created:
        CopyRecord.objects.filter(pk=instance.copy_id).update(status=CopyRecord.Status.ON_LOAN)


@receiver(pre_delete, sender=BookRecord, dispatch_uid="audit_book_delete")
def audit_book_delete(sender: type[BookRecord], instance: BookRecord, **kwargs: Any) -> None:
    audit("book_deleted", isbn=instance.isbn)


# ---------------------------------------------------------------------------
# URLs
# ---------------------------------------------------------------------------

book_patterns = (
    [
        path("", BookListView.as_view(), name="list"),
        path("new/", BookCreateView.as_view(), name="create"),
        path("<str:isbn>/", BookDetailView.as_view(), name="detail"),
        path("<str:isbn>/edit/", BookUpdateView.as_view(), name="update"),
        path("<str:isbn>/checkout/", checkout, name="checkout"),
    ],
    "books",
)

loan_patterns = (
    [
        path("", my_loans, name="mine"),
        path("overdue/", overdue, name="overdue"),
        path("<int:pk>/waive/", waive_fee, name="waive"),
    ],
    "loans",
)

urlpatterns = [
    path("admin/", admin.site.urls),
    path("books/", include(book_patterns)),
    path("loans/", include(loan_patterns, namespace="loans")),
    path("stats/", StatsView.as_view(), name="stats"),
    re_path(r"^isbn/(?P<isbn>[0-9X]{10,13})/$", BookDetailView.as_view(), name="by-isbn"),
]
