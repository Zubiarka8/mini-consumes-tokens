from django.apps import AppConfig


class LendingConfig(AppConfig):
    default_auto_field = "django.db.models.BigAutoField"
    name = "lending"
    verbose_name = "Lending desk"

    def ready(self) -> None:
        # Connects the @receiver functions; Django recommends importing the
        # signals module here rather than from models.py.
        from . import signals  # noqa: F401
