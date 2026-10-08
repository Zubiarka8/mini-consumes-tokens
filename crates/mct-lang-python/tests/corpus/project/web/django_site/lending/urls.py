from django.contrib import admin
from django.urls import include, path, re_path

from .views import (
    BookCreateView,
    BookDetailView,
    BookListView,
    BookUpdateView,
    StatsView,
    checkout,
    my_loans,
    overdue,
    waive_fee,
)

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
