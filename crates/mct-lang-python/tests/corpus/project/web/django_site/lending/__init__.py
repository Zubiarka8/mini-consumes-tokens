"""Django app for the lending library (issue #114).

Scenario: a Django 5.1 app (``django`` is an external dependency, not in
the corpus) read by ``mct-lang-python``. Owned code, one module per
responsibility as ``startapp`` lays it out: ``models``, ``admin``,
``forms``, ``views``, ``signals``, ``urls`` and ``apps``, calling into the
plain-Python ``library`` package next to it.

Expected relations: imports of the names each module uses (relative
``from .models import …`` included), calls (model field constructors,
``as_view()``, ``path()``), ``extends`` for model/view/admin bases and
mixins, and ``references`` from each decorator (``@admin.register``,
``@receiver``, ``@login_required``…) to its last name. Not extracted:
template names passed to ``render()`` and the ``urlpatterns`` entries as
routes; module-level assignments such as ``urlpatterns`` have no symbol.
"""
