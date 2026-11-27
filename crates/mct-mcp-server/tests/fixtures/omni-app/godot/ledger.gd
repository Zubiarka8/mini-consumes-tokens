class_name WorkshopLedger
extends RefCounted

func calculate_total(amount: int) -> int:
    return maxi(0, amount)
