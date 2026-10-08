from django.contrib import admin, messages
from django.http import HttpRequest

from .models import AuthorRecord, BookQuerySet, BookRecord, CopyRecord, LoanRecord


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
