"""Structural twins: same token shape, different members, callees, types or values.

Labeled evaluation corpus for the duplicate report (see ../labels.toon).
Parsed by mct-cli, never imported or run.
"""
import time


# Case py-fp-domains: a gated product over unrelated domain fields.
def order_discount(order, threshold):
    if order.subtotal < threshold:
        return 0
    amount = order.subtotal * order.discount_rate
    if order.customer.is_vip:
        amount = amount * 1.5
    return round(amount, 2)


def shipping_surcharge(parcel, limit):
    if parcel.weight < limit:
        return 0
    amount = parcel.weight * parcel.distance_factor
    if parcel.route.is_remote:
        amount = amount * 1.5
    return round(amount, 2)


# Case py-fp-effects: same loop, callees with opposite effects.
def notify_owners(project, message):
    sent = 0
    for owner in project.owners:
        if owner.active:
            send_email(owner.address, message)
            sent += 1
    audit_log("notify", project.id, sent)
    return sent


def revoke_tokens(project, reason):
    revoked = 0
    for token in project.tokens:
        if token.active:
            invalidate_token(token.value, reason)
            revoked += 1
    audit_log("revoke", project.id, revoked)
    return revoked


# Case py-fp-dataflow: equal shape, different data flow (square versus product).
def squared_error(predictions, target):
    total = 0.0
    for predicted in predictions:
        error = predicted - target
        total += error * error
    return total / len(predictions)


def cross_moment(samples, center):
    total = 0.0
    for sample in samples:
        offset = sample - center
        total += offset * sample
    return total / len(samples)


# Case py-susp-retry: one retry rule with different endpoints and policy values.
def fetch_profile(client, user_id):
    for attempt in range(3):
        response = client.get(f"/users/{user_id}", timeout=5)
        if response.status == 200:
            return response.json()
        time.sleep(0.5 * attempt)
    raise LookupError(user_id)


def fetch_invoice(client, invoice_id):
    for attempt in range(5):
        response = client.get(f"/invoices/{invoice_id}", timeout=30)
        if response.status == 200:
            return response.json()
        time.sleep(2.0 * attempt)
    raise LookupError(invoice_id)


# Case py-susp-types: one range check with different bounds and annotations.
def checked_port(value: int, default: int) -> int:
    if value is None:
        return default
    if value < 1 or value > 65535:
        raise ValueError(value)
    return value


def checked_ratio(value: float, default: float) -> float:
    if value is None:
        return default
    if value < 0 or value > 1:
        raise ValueError(value)
    return value


# Case py-amb-builders: public schema builders whose shared shape is the API convention.
def list_schema(items_schema=None, min_length=None, max_length=None, strict=None):
    return _dict_not_none(
        type="list",
        items_schema=items_schema,
        min_length=min_length,
        max_length=max_length,
        strict=strict,
    )


def set_schema(items_schema=None, min_length=None, max_length=None, strict=None):
    return _dict_not_none(
        type="set",
        items_schema=items_schema,
        min_length=min_length,
        max_length=max_length,
        strict=strict,
    )
