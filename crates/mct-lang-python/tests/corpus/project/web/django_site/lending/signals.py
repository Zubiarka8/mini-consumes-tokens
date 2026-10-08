from typing import Any

from django.db.models.signals import post_save, pre_delete
from django.dispatch import receiver

from library.errors import audit

from .models import BookRecord, CopyRecord, LoanRecord


@receiver(post_save, sender=LoanRecord)
def mark_copy_on_loan(sender: type[LoanRecord], instance: LoanRecord, created: bool, **kwargs: Any) -> None:
    if created:
        CopyRecord.objects.filter(pk=instance.copy_id).update(status=CopyRecord.Status.ON_LOAN)


@receiver(pre_delete, sender=BookRecord, dispatch_uid="audit_book_delete")
def audit_book_delete(sender: type[BookRecord], instance: BookRecord, **kwargs: Any) -> None:
    audit("book_deleted", isbn=instance.isbn)
