# Builtin names may be rebound. The detector does not execute this fixture.
int = str

def direct_a(x: int) -> int:
    value = x
    return value

def direct_b(x: str) -> str:
    value = x
    return value

def direct_c(x: Custom) -> Custom:
    value = x
    return value

def compound_a(x: list[int]) -> list[int]:
    return x

def compound_b(x: list[str]) -> list[str]:
    return x
