package billing

// Shape is satisfied structurally — no type in this fixture declares
// "implements Shape" anywhere, because Go has no such keyword. Whether
// Invoice (or anything else) satisfies it is a compiler-only fact this
// index deliberately does not attempt to resolve; see the
// `find_implementations` deferral note in mct-lang-go's module doc.
type Shape interface {
	Area() float64
	Perimeter() float64
}
