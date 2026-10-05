from __future__ import annotations

from datetime import timedelta
from typing import Any

from django.conf import settings
from django.core.exceptions import ValidationError as DjangoValidationError
from django.db import models
from django.db.models import Count, Q
from django.urls import reverse
from django.utils import timezone

from library.models import Genre, is_valid_isbn, normalize_isbn, overdue_fee

LOAN_PERIOD = timedelta(days=21)


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
