def repeated(a):
    def inner(a):
        return a + a
    return inner(a)

def renamed(x):
    def nested(x):
        return x + x
    return nested(x)

def distinct(x):
    def nested(y):
        return y + y
    return nested(x)
