from django import forms

from library.models import is_valid_isbn, normalize_isbn

from .models import BookRecord


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
