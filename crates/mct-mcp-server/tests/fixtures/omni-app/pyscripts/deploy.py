from notify import send_alert, format_alert


def deploy(account, total):
    body = format_alert(account, total)
    return send_alert("ops", body)


class Deployer:
    def run(self, account):
        return deploy(account, 0)
