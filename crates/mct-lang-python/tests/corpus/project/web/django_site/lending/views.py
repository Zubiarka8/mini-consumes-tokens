from __future__ import annotations

from typing import Any

from django.conf import settings
from django.contrib import messages
from django.contrib.auth.decorators import login_required, permission_required
from django.contrib.auth.mixins import LoginRequiredMixin, PermissionRequiredMixin
from django.db import transaction
from django.db.models import Count, F
from django.http import HttpRequest, HttpResponse, JsonResponse
from django.shortcuts import get_object_or_404, redirect, render
from django.utils import timezone
from django.views import View
from django.views.decorators.cache import cache_page
from django.views.decorators.http import require_GET, require_POST
from django.views.generic import CreateView, DetailView, ListView, UpdateView

from library.errors import LimitExceededError, audit
from library.models import Genre, normalize_isbn
from library.services import build_services

from .forms import BookForm, CheckoutForm
from .models import BookQuerySet, BookRecord, CopyRecord, LoanRecord


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
    permission_required = "lending.add_bookrecord"


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
@permission_required("lending.waive_fee", raise_exception=True)
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


@login_required
def my_loans(request: HttpRequest) -> HttpResponse:
    loans = LoanRecord.objects.filter(member=request.user).select_related("copy__book")
    return render(request, "loans/mine.html", {"loans": loans})
