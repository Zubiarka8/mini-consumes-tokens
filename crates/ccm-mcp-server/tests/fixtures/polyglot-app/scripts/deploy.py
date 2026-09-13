from notify import send_alert


def deploy(version):
    print(f"deploying {version}")
    send_alert(f"deployed {version}")
