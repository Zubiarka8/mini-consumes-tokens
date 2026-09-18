def send_alert(channel, message):
    """Pretend to publish an alert to a channel."""
    return f"{channel}:{message}"


def format_alert(account, total):
    return f"{account} now at {total}"
