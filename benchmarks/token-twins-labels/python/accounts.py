"""Account handlers that repeat a validation block but differ around it.

Labeled evaluation corpus for the duplicate report (see ../labels.toon).
Parsed by mct-cli, never imported or run.
"""
import logging
import time

logger = logging.getLogger(__name__)
REQUIRED_FIELDS = ("email", "name")


# Case py-block-validate: lines 15-22 and 32-39 are one repeated block.
def create_account(payload, repository):
    if not isinstance(payload, dict):
        raise TypeError("payload must be a mapping")
    missing = [field for field in REQUIRED_FIELDS if field not in payload]
    if missing:
        raise ValueError("missing fields: " + ", ".join(missing))
    email = payload["email"].strip().lower()
    if "@" not in email:
        raise ValueError("invalid email: " + email)
    record = repository.insert(email=email, name=payload["name"])
    repository.commit()
    return record.id


def update_account(account_id, payload, repository):
    account = repository.get(account_id)
    if account is None:
        raise KeyError(account_id)
    if not isinstance(payload, dict):
        raise TypeError("payload must be a mapping")
    missing = [field for field in REQUIRED_FIELDS if field not in payload]
    if missing:
        raise ValueError("missing fields: " + ", ".join(missing))
    email = payload["email"].strip().lower()
    if "@" not in email:
        raise ValueError("invalid email: " + email)
    account.email = email
    account.name = payload.get("name", account.name)
    repository.save(account)
    return account.version


# Case py-nearmiss-timing: a three-line timing snippet repeated in two
# otherwise different functions. Too small to report as a partial block.
def rebuild_search_index(documents, index):
    started = time.monotonic()
    index.clear()
    for document in documents:
        index.add(document.id, document.title, document.body)
    index.flush()
    elapsed = time.monotonic() - started
    logger.debug("took %.3f s", elapsed)
    return len(documents)


def purge_expired_sessions(store, now):
    started = time.monotonic()
    expired = [key for key, session in store.items() if session.expires_at < now]
    for key in expired:
        del store[key]
    elapsed = time.monotonic() - started
    logger.debug("took %.3f s", elapsed)
    return expired
