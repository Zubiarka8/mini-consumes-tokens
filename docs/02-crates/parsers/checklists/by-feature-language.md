# Parser checklists by feature and language

> **Verification status:** checked on 2026-11-04 with `mct-cli` (debug build, commit `bb5eb03`) against
> probe fixtures. `[?]` means the feature was not exercised in that check; it does not mean the feature is
> missing. The earlier record is the 2026-10-19 probe at commit `71ec6d0`.

Primary language view. Each feature lists all 17 languages in the same order: a checkbox, or a named
not-applicable reason. Definitions are in [features.md](../features.md). Same data as
[by-language.md](by-language.md). Library features are tracked separately in
[by-feature-library.md](by-feature-library.md).

Marks:

- `[x]` done
- `[~]` partial: works only in some forms; the note says which
- `[ ]` not done
- `[?]` not verified

## Functions and methods

### FN01 Top-level function

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 9 | 1 | 0 | 0 | 7 |

- [x] Rust: `function`
- [x] Python: `async def` too
- [x] JS/TS: `function`
- — Java: no free functions
- [x] Kotlin: `function`
- — C#: methods only; local functions are FN02
- [x] C++: `p_helper`; namespace functions
- [x] Go: parent is the package
- [x] PHP: `function`
- [x] Lua: `function`
- [x] Bash: `f() {}`
- [~] PowerShell: multi-line works; one-line `function f { param(...) … }` is a syntax error
- — HTML: HTML is markup; it has no top-level function
- — CSS: CSS is markup; it has no top-level function
- — XML: XML is markup; it has no top-level function
- — XAML: XAML is markup; it has no top-level function
- — Markdown: Markdown is markup; it has no top-level function

*Done when:* Free function at file or module top level, indexed as `function`.


### FN02 Nested function

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 3 | 6 | 0 | 0 | 8 |

- [x] Rust: indexed as a function with its enclosing callable as parent
- [~] Python: file skeleton shows `nested` indented, but the parser test confirms `parent` is `None`
- [~] JS/TS: nested function is indexed, but parser test confirms `parent` is `None`
- — Java: no nested named functions
- [~] Kotlin: local `inner` function is indexed without a parent association
- [~] C#: local function → `function`, parent not checked
- — C++: C++ has no nested named functions
- — Go: Go has no nested function
- [~] PHP: named `nestedNamed` function is indexed without a parent association
- [~] Lua: local `nested` function is indexed without a parent association
- [x] Bash: parent is the outer function
- [x] PowerShell: parent is the outer function
- — HTML: HTML is markup; it has no nested function
- — CSS: CSS is markup; it has no nested function
- — XML: XML is markup; it has no nested function
- — XAML: XAML is markup; it has no nested function
- — Markdown: Markdown is markup; it has no nested function

*Done when:* Named function inside another function, indexed as `function` with the outer function as parent.


### FN03 Lambda or closure assigned to a name

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 3 | 2 | 6 | 0 | 6 |

- [~] Rust: simple `let name = |...|` closures are indexed as functions; other assignment forms are not
- [ ] Python: `find_symbol("assigned_lambda")` returns no symbol
- [x] JS/TS: `const p_arrow = …` → `function`
- [ ] Java: lambda `r` has no symbol
- [ ] Kotlin: `val f = { … }` has no symbol
- [ ] C#: assigned lambda variable `f` has no symbol (MCT probe)
- [ ] C++: lambda `l` has no symbol
- [ ] Go: `f := func()` has no symbol
- [x] PHP: `$f = function` → `function`
- [x] Lua: `local p_lam = function`
- — Bash: Bash has no lambda or closure assigned to a name
- [~] PowerShell: top-level `$f = { … }` is a `variable`
- — HTML: HTML is markup; it has no lambda or closure assigned to a name
- — CSS: CSS is markup; it has no lambda or closure assigned to a name
- — XML: XML is markup; it has no lambda or closure assigned to a name
- — XAML: XAML is markup; it has no lambda or closure assigned to a name
- — Markdown: Markdown is markup; it has no lambda or closure assigned to a name

*Done when:* `f = <lambda>` or `f := func…` gives a symbol, not only a `calls` relation.


### FN04 Method

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 10 | 0 | 1 | 0 | 6 |

- [x] Rust: parent is the impl type
- [x] Python: parent is the class
- [x] JS/TS: `p_m` as `method`
- [x] Java: `method`
- [x] Kotlin: extension `String.p_ext` → `method`
- [x] C#: methods as `method`
- [x] C++: out-of-line member has its class as parent
- [x] Go: pointer receiver as `method`
- [x] PHP: `method`
- [x] Lua: `M.p_mod` and `M:p_meth` as `method`
- — Bash: Bash has no method
- [ ] PowerShell: PowerShell classes produce no symbols (corpus test)
- — HTML: HTML is markup; it has no method
- — CSS: CSS is markup; it has no method
- — XML: XML is markup; it has no method
- — XAML: XAML is markup; it has no method
- — Markdown: Markdown is markup; it has no method

*Done when:* Function declared in a class, struct, impl or table, indexed as `method` with its owner as parent.


### FN05 Constructor

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 6 | 0 | 2 | 0 | 9 |


- — Rust: no constructor keyword; `fn new` is a method
- [x] Python: `__init__`
- [x] JS/TS: `constructor` as `method`
- [x] Java: constructor as `method`
- [ ] Kotlin: secondary constructor has no symbol
- [x] C#: constructor `P_Base`
- [x] C++: constructor `P_Base` as `method`
- — Go: no constructor keyword; `NewX` is a function
- [x] PHP: `__construct`
- — Lua: no constructor keyword
- — Bash: Bash has no constructor
- [ ] PowerShell: PowerShell classes produce no symbols (corpus test)
- — HTML: HTML is markup; it has no constructor
- — CSS: CSS is markup; it has no constructor
- — XML: XML is markup; it has no constructor
- — XAML: XAML is markup; it has no constructor
- — Markdown: Markdown is markup; it has no constructor

*Done when:* Constructor indexed as `method`, named after the class or as a special name.


### FN06 Destructor / finalizer

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 5 | 0 | 0 | 0 | 12 |

- [x] Rust: `drop` is a `method` owned by `Widget`
- [x] Python: `__del__`
- — JS/TS: JS/TS has no destructor / finalizer
- — Java: Java has no destructor / finalizer
- — Kotlin: Kotlin has no destructor / finalizer
- [x] C#: `~P_Base`
- [x] C++: `~P_Base`
- — Go: Go has no destructor / finalizer
- [x] PHP: `__destruct`
- — Lua: Lua has no destructor / finalizer
- — Bash: Bash has no destructor / finalizer
- — PowerShell: PowerShell has no destructor / finalizer
- — HTML: HTML is markup; it has no destructor / finalizer
- — CSS: CSS is markup; it has no destructor / finalizer
- — XML: XML is markup; it has no destructor / finalizer
- — XAML: XAML is markup; it has no destructor / finalizer
- — Markdown: Markdown is markup; it has no destructor / finalizer

*Done when:* Destructor or finalizer indexed as `method`.


### FN07 Operator overload

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 5 | 1 | 0 | 0 | 11 |

- [x] Rust: `add` is a `method` owned by `Widget`
- [x] Python: `__add__`
- — JS/TS: JS/TS has no operator overload
- — Java: Java has no operator overload
- [x] Kotlin: `operator fun plus`
- [x] C#: `operator +`
- [x] C++: `operator+`
- — Go: Go has no operator overload
- — PHP: PHP has no operator overload
- [~] Lua: `__add` is indexed as a method; no operator relation is emitted
- — Bash: Bash has no operator overload
- — PowerShell: PowerShell has no operator overload
- — HTML: HTML is markup; it has no operator overload
- — CSS: CSS is markup; it has no operator overload
- — XML: XML is markup; it has no operator overload
- — XAML: XAML is markup; it has no operator overload
- — Markdown: Markdown is markup; it has no operator overload

*Done when:* Operator implementation indexed as `method`.


### FN08 Abstract / pure virtual method

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 7 | 0 | 1 | 0 | 9 |

- [x] Rust: required trait method is indexed as `method` in `ChildTrait`
- [x] Python: `@abstractmethod`
- [ ] JS/TS: `abstract p_am(): void;` has no symbol
- [x] Java: abstract `p_am`
- [x] Kotlin: `abstract fun p_am`
- [x] C#: `abstract P_Am`
- [x] C++: `= 0` method
- — Go: Go has no abstract / pure virtual method
- [x] PHP: abstract `p_am`
- — Lua: Lua has no abstract / pure virtual method
- — Bash: Bash has no abstract / pure virtual method
- — PowerShell: PowerShell has no abstract / pure virtual method
- — HTML: HTML is markup; it has no abstract / pure virtual method
- — CSS: CSS is markup; it has no abstract / pure virtual method
- — XML: XML is markup; it has no abstract / pure virtual method
- — XAML: XAML is markup; it has no abstract / pure virtual method
- — Markdown: Markdown is markup; it has no abstract / pure virtual method

*Done when:* Method without a body (abstract, pure virtual, interface) indexed as `method`.


### FN09 Property accessors (get/set)

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 2 | 2 | 1 | 0 | 12 |


- — Rust: Rust has no property accessors (get/set)
- [~] Python: getter and setter are two `p_val` rows
- [~] JS/TS: `get` and `set` are two rows
- — Java: getters are ordinary methods
- [x] Kotlin: one `field`
- [x] C#: property `P_Prop` as `field`
- — C++: C++ has no property accessors (get/set)
- — Go: Go has no property accessors (get/set)
- — PHP: PHP has no property accessors (get/set)
- — Lua: Lua has no property accessors (get/set)
- — Bash: Bash has no property accessors (get/set)
- [ ] PowerShell: PowerShell classes produce no symbols (corpus test)
- — HTML: HTML is markup; it has no property accessors (get/set)
- — CSS: CSS is markup; it has no property accessors (get/set)
- — XML: XML is markup; it has no property accessors (get/set)
- — XAML: XAML is markup; it has no property accessors (get/set)
- — Markdown: Markdown is markup; it has no property accessors (get/set)

*Done when:* A getter and setter give one symbol, or two rows that are documented as the same property.


### FN10 Overloads kept apart

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 4 | 1 | 0 | 0 | 12 |


- — Rust: Rust has no overloads kept apart
- — Python: Python has no overloads kept apart
- [~] JS/TS: overload signatures dropped; implementation kept
- [x] Java: two `p_m` rows
- [x] Kotlin: two `p_ov` rows
- [x] C#: two `P_Ov` rows
- [x] C++: two overloads of `overloaded` are retained (MCT probe)
- — Go: Go has no overloads kept apart
- — PHP: PHP has no overloads kept apart
- — Lua: Lua has no overloads kept apart
- — Bash: Bash has no overloads kept apart
- — PowerShell: PowerShell has no overloads kept apart
- — HTML: HTML is markup; it has no overloads kept apart
- — CSS: CSS is markup; it has no overloads kept apart
- — XML: XML is markup; it has no overloads kept apart
- — XAML: XAML is markup; it has no overloads kept apart
- — Markdown: Markdown is markup; it has no overloads kept apart

*Done when:* Each overload is its own row with the same name and parent.


## Types and object-oriented programming

### TY01 Class

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 8 | 0 | 1 | 0 | 8 |


- — Rust: Rust has no class
- [x] Python: `class`
- [x] JS/TS: `class`
- [x] Java: `class`
- [x] Kotlin: `class`
- [x] C#: `class`
- [x] C++: `class`
- — Go: Go has no class
- [x] PHP: `class`
- [x] Lua: `local Widget = setmetatable({}, …)` → `class` (2026-11 record; the 2026-10 record said no symbol)
- — Bash: Bash has no class
- [ ] PowerShell: PowerShell classes produce no symbols (corpus test)
- — HTML: HTML is markup; it has no class
- — CSS: CSS is markup; it has no class
- — XML: XML is markup; it has no class
- — XAML: XAML is markup; it has no class
- — Markdown: Markdown is markup; it has no class

*Done when:* Class indexed as `class`.


### TY02 Abstract class

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 6 | 1 | 0 | 0 | 10 |


- — Rust: Rust has no abstract class
- [x] Python: abstract flag not stored
- [x] JS/TS: `abstract class` → `class`
- [x] Java: `abstract static class` → `class`
- [x] Kotlin: `abstract class` → `class`
- [x] C#: abstract class → `class`
- [~] C++: pure-virtual `Base` is a `struct` symbol; abstractness is not represented
- — Go: Go has no abstract class
- [x] PHP: `abstract class` → `class`
- — Lua: Lua has no abstract class
- — Bash: Bash has no abstract class
- — PowerShell: PowerShell has no abstract class
- — HTML: HTML is markup; it has no abstract class
- — CSS: CSS is markup; it has no abstract class
- — XML: XML is markup; it has no abstract class
- — XAML: XAML is markup; it has no abstract class
- — Markdown: Markdown is markup; it has no abstract class

*Done when:* Abstract class indexed as `class` (the abstract flag is not stored).


### TY03 Struct

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 4 | 0 | 0 | 0 | 13 |

- [x] Rust: `struct`
- — Python: Python has no struct
- — JS/TS: JS/TS has no struct
- — Java: Java has no struct
- — Kotlin: Kotlin has no struct
- [x] C#: `struct`
- [x] C++: `struct`
- [x] Go: `struct`
- — PHP: PHP has no struct
- — Lua: Lua has no struct
- — Bash: Bash has no struct
- — PowerShell: PowerShell has no struct
- — HTML: HTML is markup; it has no struct
- — CSS: CSS is markup; it has no struct
- — XML: XML is markup; it has no struct
- — XAML: XAML is markup; it has no struct
- — Markdown: Markdown is markup; it has no struct

*Done when:* Struct indexed as `struct`.


### TY04 Union

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 2 | 0 | 0 | 15 |

- [~] Rust: named unions are indexed as `struct` because the common model has no union kind
- — Python: Python has no union
- — JS/TS: JS/TS has no union
- — Java: Java has no union
- — Kotlin: Kotlin has no union
- — C#: C# has no union
- [~] C++: `P_U` indexed as `struct`
- — Go: Go has no union
- — PHP: PHP has no union
- — Lua: Lua has no union
- — Bash: Bash has no union
- — PowerShell: PowerShell has no union
- — HTML: HTML is markup; it has no union
- — CSS: CSS is markup; it has no union
- — XML: XML is markup; it has no union
- — XAML: XAML is markup; it has no union
- — Markdown: Markdown is markup; it has no union

*Done when:* Union indexed as its own kind or as `struct`, with its members.


### TY05 Interface / protocol

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 6 | 0 | 0 | 0 | 11 |


- — Rust: traits are TY07
- — Python: `typing.Protocol` is a class (TY01)
- [x] JS/TS: `interface`
- [x] Java: `interface`
- [x] Kotlin: `interface`
- [x] C#: `interface`
- — C++: C++ has no interface / protocol
- [x] Go: `interface`
- [x] PHP: `interface`
- — Lua: Lua has no interface / protocol
- — Bash: Bash has no interface / protocol
- — PowerShell: PowerShell has no interface / protocol
- — HTML: HTML is markup; it has no interface / protocol
- — CSS: CSS is markup; it has no interface / protocol
- — XML: XML is markup; it has no interface / protocol
- — XAML: XAML is markup; it has no interface / protocol
- — Markdown: Markdown is markup; it has no interface / protocol

*Done when:* Interface indexed as `interface`.


### TY06 Interface members

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 6 | 0 | 1 | 0 | 10 |

- [x] Rust: trait method as `method`
- — Python: Python has no interface members
- [ ] JS/TS: `P_Iface.p_im` has no symbol; only `Impl.p_im` is indexed
- [x] Java: `p_im` in interface
- [x] Kotlin: `fun p_im()` in interface
- [x] C#: methods as `method`
- — C++: C++ has no interface members
- [x] Go: `P_Im` in interface
- [x] PHP: interface method
- — Lua: Lua has no interface members
- — Bash: Bash has no interface members
- — PowerShell: PowerShell has no interface members
- — HTML: HTML is markup; it has no interface members
- — CSS: CSS is markup; it has no interface members
- — XML: XML is markup; it has no interface members
- — XAML: XAML is markup; it has no interface members
- — Markdown: Markdown is markup; it has no interface members

*Done when:* Methods and properties inside an interface indexed as `method` or `field`.


### TY07 Trait / mixin

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 2 | 1 | 0 | 0 | 14 |

- [x] Rust: `trait`
- — Python: Python has no trait / mixin
- [~] JS/TS: mixin factory and method are indexed, but the method has no mixin owner
- — Java: Java has no trait / mixin
- — Kotlin: Kotlin has no trait / mixin
- — C#: C# has no trait / mixin
- — C++: C++ has no trait / mixin
- — Go: Go has no trait / mixin
- [x] PHP: `trait` with its methods
- — Lua: Lua has no trait / mixin
- — Bash: Bash has no trait / mixin
- — PowerShell: PowerShell has no trait / mixin
- — HTML: HTML is markup; it has no trait / mixin
- — CSS: CSS is markup; it has no trait / mixin
- — XML: XML is markup; it has no trait / mixin
- — XAML: XAML is markup; it has no trait / mixin
- — Markdown: Markdown is markup; it has no trait / mixin

*Done when:* Trait or mixin indexed as `trait`, or as the closest kind, with its members.


### TY08 Enum

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 6 | 2 | 1 | 0 | 8 |

- [x] Rust: `enum`
- [~] Python: `Enum` subclass is a `class`
- [x] JS/TS: `enum`
- [x] Java: `enum`
- [~] Kotlin: `enum class` is a `class`
- [x] C#: `enum`
- [x] C++: `enum` and `enum class`
- — Go: Go has no enum
- [x] PHP: `enum`
- — Lua: Lua has no enum
- — Bash: Bash has no enum
- [ ] PowerShell: PowerShell enum declarations produce no symbols (corpus test)
- — HTML: HTML is markup; it has no enum
- — CSS: CSS is markup; it has no enum
- — XML: XML is markup; it has no enum
- — XAML: XAML is markup; it has no enum
- — Markdown: Markdown is markup; it has no enum

*Done when:* Enum indexed as `enum`.


### TY09 Enum members

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 6 | 0 | 3 | 0 | 8 |

- [x] Rust: variants are indexed as constants owned by the enum
- [ ] Python: `find_symbol("RED")` returns no symbol
- [ ] JS/TS: `find_symbol("Red")` returns no symbol
- [x] Java: enum constants as `field`
- [x] Kotlin: entries as `field`
- [x] C#: members as `constant`
- [x] C++: values as `constant`
- — Go: Go has no enum members
- [x] PHP: `case` as `field`
- — Lua: Lua has no enum members
- — Bash: Bash has no enum members
- [ ] PowerShell: PowerShell enum declarations produce no symbols (corpus test)
- — HTML: HTML is markup; it has no enum members
- — CSS: CSS is markup; it has no enum members
- — XML: XML is markup; it has no enum members
- — XAML: XAML is markup; it has no enum members
- — Markdown: Markdown is markup; it has no enum members

*Done when:* Enum values indexed as `field` or `constant`.


### TY10 Record / data class

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 3 | 0 | 0 | 13 |


- — Rust: Rust has no record / data class
- [~] Python: `Record` is a `class`; `field` has no symbol
- — JS/TS: JS/TS has no record / data class
- [~] Java: `record` is a `class`; `p_comp` has no symbol
- [x] Kotlin: `data class` → `class`; `val` parameter → `field`
- [~] C#: `record` is a `class`; `P_Comp` has no symbol
- — C++: C++ has no record / data class
- — Go: Go has no record / data class
- — PHP: PHP has no record / data class
- — Lua: Lua has no record / data class
- — Bash: Bash has no record / data class
- — PowerShell: PowerShell has no record / data class
- — HTML: HTML is markup; it has no record / data class
- — CSS: CSS is markup; it has no record / data class
- — XML: XML is markup; it has no record / data class
- — XAML: XAML is markup; it has no record / data class
- — Markdown: Markdown is markup; it has no record / data class

*Done when:* Record or data class indexed as `class`, with its components as `field`.


### TY11 Nested / inner class

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 6 | 0 | 0 | 0 | 11 |


- — Rust: Rust has no nested / inner class
- [x] Python: parent is the outer class
- [x] JS/TS: `Inner` is indexed with `Outer` as parent
- [x] Java: static and inner
- [x] Kotlin: `P_Nested`, `P_In`
- [x] C#: parent is the outer class
- [x] C++: nested `P_Nested`
- — Go: Go has no nested / inner class
- — PHP: PHP has no nested class syntax; the [nested-class RFC was declined](https://wiki.php.net/rfc/short-and-inner-classes)
- — Lua: Lua has no nested / inner class
- — Bash: Bash has no nested / inner class
- — PowerShell: PowerShell has no nested / inner class
- — HTML: HTML is markup; it has no nested / inner class
- — CSS: CSS is markup; it has no nested / inner class
- — XML: XML is markup; it has no nested / inner class
- — XAML: XAML is markup; it has no nested / inner class
- — Markdown: Markdown is markup; it has no nested / inner class

*Done when:* Class declared inside another, with the outer class as parent.


### TY12 Anonymous class

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 2 | 3 | 0 | 12 |


- — Rust: Rust has no anonymous class
- — Python: Python has no anonymous class
- [ ] JS/TS: anonymous class has no class symbol; `anonymousMethod` has no owner
- [~] Java: method inside a method → enclosing class; field initializer → no symbol
- [~] Kotlin: `object : P_Abs()` gives no class; its function has no parent
- [ ] C#: anonymous type `new { X = 1 }` has no symbol
- — C++: C++ has no anonymous class
- — Go: Go has no anonymous class
- [ ] PHP: `new class {}` gives no symbols
- — Lua: Lua has no anonymous class
- — Bash: Bash has no anonymous class
- — PowerShell: PowerShell has no anonymous class
- — HTML: HTML is markup; it has no anonymous class
- — CSS: CSS is markup; it has no anonymous class
- — XML: XML is markup; it has no anonymous class
- — XAML: XAML is markup; it has no anonymous class
- — Markdown: Markdown is markup; it has no anonymous class

*Done when:* Anonymous class indexed with its members attached to it.


### TY13 Partial class

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 0 | 0 | 0 | 16 |


- — Rust: Rust has no partial class
- — Python: Python has no partial class
- — JS/TS: JS/TS has no partial class
- — Java: Java has no partial class
- — Kotlin: Kotlin has no partial class
- [x] C#: one row per declaration
- — C++: C++ has no partial class
- — Go: Go has no partial class
- — PHP: PHP has no partial class
- — Lua: Lua has no partial class
- — Bash: Bash has no partial class
- — PowerShell: PowerShell has no partial class
- — HTML: HTML is markup; it has no partial class
- — CSS: CSS is markup; it has no partial class
- — XML: XML is markup; it has no partial class
- — XAML: XAML is markup; it has no partial class
- — Markdown: Markdown is markup; it has no partial class

*Done when:* Each declaration of a partial class gives a row; members keep the class as parent.


### TY14 Singleton / companion object

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 1 | 0 | 0 | 16 |


- — Rust: Rust has no singleton / companion object
- — Python: Python has no singleton / companion object
- — JS/TS: JS/TS has no singleton / companion object
- — Java: Java has no singleton / companion object
- [~] Kotlin: `companion object` has no symbol; `P_COMP` is a `field` of the outer class
- — C#: C# has no singleton / companion object
- — C++: C++ has no singleton / companion object
- — Go: Go has no singleton / companion object
- — PHP: PHP has no singleton / companion object
- — Lua: Lua has no singleton / companion object
- — Bash: Bash has no singleton / companion object
- — PowerShell: PowerShell has no singleton / companion object
- — HTML: HTML is markup; it has no singleton / companion object
- — CSS: CSS is markup; it has no singleton / companion object
- — XML: XML is markup; it has no singleton / companion object
- — XAML: XAML is markup; it has no singleton / companion object
- — Markdown: Markdown is markup; it has no singleton / companion object

*Done when:* Singleton or companion object indexed, with its members under it.


### TY15 Type alias

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 5 | 1 | 1 | 0 | 10 |

- [x] Rust: `type` alias
- [x] Python: PEP 695 `type X = …`
- [x] JS/TS: `type` alias
- — Java: Java has no type alias
- [ ] Kotlin: `typealias` has no symbol
- [~] C#: `delegate` → `type_alias`; `using` alias emits imports
- [x] C++: `using` and `typedef`
- [x] Go: both alias forms
- — PHP: PHP has no type alias
- — Lua: Lua has no type alias
- — Bash: Bash has no type alias
- — PowerShell: PowerShell has no type alias
- — HTML: HTML is markup; it has no type alias
- — CSS: CSS is markup; it has no type alias
- — XML: XML is markup; it has no type alias
- — XAML: XAML is markup; it has no type alias
- — Markdown: Markdown is markup; it has no type alias

*Done when:* Alias indexed as `type_alias`.


### TY16 Module / namespace / package

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 9 | 3 | 0 | 0 | 5 |

- [x] Rust: file module plus `mod` blocks
- [x] Python: file `module`
- [x] JS/TS: file module plus `namespace`
- [~] Java: `package` has no symbol
- [x] Kotlin: `package` module
- [x] C#: `namespace P.Ns` → `module`
- [x] C++: `namespace pns` → `module`
- [~] Go: package `module` and file module; same name only when the file is named after the package
- [~] PHP: `namespace` is not a symbol
- [x] Lua: file `module` and table `module`
- [x] Bash: file `module`
- [x] PowerShell: file `module`
- — HTML: HTML is markup; it has no module / namespace / package
- — CSS: CSS is markup; it has no module / namespace / package
- — XML: XML is markup; it has no module / namespace / package
- — XAML: XAML is markup; it has no module / namespace / package
- — Markdown: Markdown is markup; it has no module / namespace / package

*Done when:* File-level `module` row, plus namespace or package blocks where the language has them.


## Fields, constants and variables

### DA01 Field / instance attribute

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 7 | 0 | 4 | 0 | 6 |

- [x] Rust: named fields and tuple positions are indexed as fields
- [ ] Python: `self.value` is omitted from the field-symbol query
- [x] JS/TS: `#p_priv` and `static` fields
- [x] Java: `field`
- [x] Kotlin: `var p_prop`
- [x] C#: fields, properties and events as `field`
- [x] C++: `p_field` and union members
- [ ] Go: struct fields have no symbol
- [x] PHP: property `$prop`
- [ ] Lua: `self.x` produces no field symbol (MCT probe)
- — Bash: Bash has no field / instance attribute
- [ ] PowerShell: class declarations and properties are not indexed (corpus test)
- — HTML: HTML is markup; it has no field / instance attribute
- — CSS: CSS is markup; it has no field / instance attribute
- — XML: XML is markup; it has no field / instance attribute
- — XAML: XAML is markup; it has no field / instance attribute
- — Markdown: Markdown is markup; it has no field / instance attribute

*Done when:* Instance or class field indexed as `field`.


### DA02 Constructor-promoted field

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 2 | 0 | 2 | 0 | 13 |


- — Rust: Rust has no constructor-promoted field
- — Python: Python has no constructor-promoted field
- [ ] JS/TS: `constructor(public p_promo)` has no symbol
- — Java: Java has no constructor-promoted field
- [x] Kotlin: `val p_promo` → `field`
- [ ] C#: record positional parameter has no symbol
- — C++: C++ has no constructor-promoted field
- — Go: Go has no constructor-promoted field
- [x] PHP: promoted parameter → `field`
- — Lua: Lua has no constructor-promoted field
- — Bash: Bash has no constructor-promoted field
- — PowerShell: PowerShell has no constructor-promoted field
- — HTML: HTML is markup; it has no constructor-promoted field
- — CSS: CSS is markup; it has no constructor-promoted field
- — XML: XML is markup; it has no constructor-promoted field
- — XAML: XAML is markup; it has no constructor-promoted field
- — Markdown: Markdown is markup; it has no constructor-promoted field

*Done when:* Constructor parameter that becomes a field (`val`/`var`, `public x`) indexed as `field`.


### DA03 Constant (module or global)

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 2 | 4 | 4 | 0 | 7 |

- [x] Rust: `const` → `constant`
- [ ] Python: `P_CONST` has no symbol
- [ ] JS/TS: `const P_CONST` has no symbol
- [~] Java: `static final` is a `field`
- [ ] Kotlin: top-level `const val` has no symbol
- [~] C#: `const` is a `field`
- [~] C++: `const int P_CONST` → `variable`
- [ ] Go: `const` has no symbol
- [x] PHP: `const` top-level and in classes
- — Lua: no `const` keyword
- [~] Bash: `readonly X=…` → `variable`
- — PowerShell: PowerShell has no constant (module or global)
- — HTML: HTML is markup; it has no constant (module or global)
- — CSS: CSS is markup; it has no constant (module or global)
- — XML: XML is markup; it has no constant (module or global)
- — XAML: XAML is markup; it has no constant (module or global)
- — Markdown: Markdown is markup; it has no constant (module or global)

*Done when:* Constant indexed as `constant`.


### DA04 Global / module-level variable

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 4 | 1 | 5 | 0 | 7 |

- [x] Rust: `static` declarations are indexed as variables
- [ ] Python: `P_MOD` has no symbol
- [ ] JS/TS: `let p_var` has no symbol
- — Java: Java has no global / module-level variable
- [ ] Kotlin: top-level `val` has no symbol
- — C#: C# has no global / module-level variable
- [x] C++: `p_global` → `variable`
- [ ] Go: `var` has no symbol
- [ ] PHP: `$p_glob = …` has no symbol
- [~] Lua: global `MSG = …` → `variable`; top-level `local` not exercised
- [x] Bash: `P_GLOBAL` → `variable`
- [x] PowerShell: scope prefixes stripped
- — HTML: HTML is markup; it has no global / module-level variable
- — CSS: CSS is markup; it has no global / module-level variable
- — XML: XML is markup; it has no global / module-level variable
- — XAML: XAML is markup; it has no global / module-level variable
- — Markdown: Markdown is markup; it has no global / module-level variable

*Done when:* Module-level or global variable indexed as `variable`.


### DA05 Local variable

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 1 | 10 | 0 | 5 |

- [~] Rust: `let` bindings, including destructuring, are indexed; other binding forms are not covered
- [ ] Python: `find_symbol("local_value")` returns no symbol
- [ ] JS/TS: `find_symbol("local_value")` returns no symbol
- [ ] Java: lambda local not indexed
- [ ] Kotlin: `find_symbol(local)` returns no symbol
- [ ] C#: `find_symbol(local)` returns no symbol
- [ ] C++: `auto l` not indexed
- [ ] Go: `find_symbol(local)` returns no symbol
- [ ] PHP: `find_symbol(local)` returns no symbol
- [x] Lua: `find_symbol(local_value)` returns a variable under `method`
- [ ] Bash: `local` is not indexed (by design)
- [ ] PowerShell: corpus test confirms function-local assignments are not indexed
- — HTML: HTML is markup; it has no local variable
- — CSS: CSS is markup; it has no local variable
- — XML: XML is markup; it has no local variable
- — XAML: XAML is markup; it has no local variable
- — Markdown: Markdown is markup; it has no local variable

*Done when:* Variable inside a function indexed as a symbol.


### DA06 Parameter

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 0 | 10 | 0 | 6 |

- [x] Rust: function and closure parameters are indexed as variables
- [ ] Python: `find_symbol("argument")` returns no symbol
- [ ] JS/TS: `find_symbol("inner")` returns no symbol
- [ ] Java: parameter not indexed
- [ ] Kotlin: `find_symbol(input)` returns no symbol
- [ ] C#: `find_symbol(input)` returns no symbol
- [ ] C++: `find_symbol(input)` returns no symbol
- [ ] Go: `find_symbol(input)` returns no symbol
- [ ] PHP: `find_symbol(input)` returns no symbol
- [ ] Lua: `find_symbol(input)` returns no symbol
- — Bash: Bash has no parameter
- [ ] PowerShell: no parameter symbol was found in the parser corpus
- — HTML: HTML is markup; it has no parameter
- — CSS: CSS is markup; it has no parameter
- — XML: XML is markup; it has no parameter
- — XAML: XAML is markup; it has no parameter
- — Markdown: Markdown is markup; it has no parameter

*Done when:* Function parameter indexed as a symbol.


### DA07 Macro / preprocessor define

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 1 | 1 | 0 | 15 |

- [~] Rust: `macro_rules!` definitions are indexed as functions; macro expansion and other forms are not covered
- — Python: Python has no macro / preprocessor define
- — JS/TS: JS/TS has no macro / preprocessor define
- — Java: Java has no macro / preprocessor define
- — Kotlin: Kotlin has no macro / preprocessor define
- — C#: C# has no macro / preprocessor define
- [ ] C++: `#define P_MACRO` has no symbol
- — Go: Go has no macro / preprocessor define
- — PHP: `define()` stays a call by decision
- — Lua: Lua has no macro / preprocessor define
- — Bash: Bash has no macro / preprocessor define
- — PowerShell: PowerShell has no macro / preprocessor define
- — HTML: HTML is markup; it has no macro / preprocessor define
- — CSS: CSS is markup; it has no macro / preprocessor define
- — XML: XML is markup; it has no macro / preprocessor define
- — XAML: XAML is markup; it has no macro / preprocessor define
- — Markdown: Markdown is markup; it has no macro / preprocessor define

*Done when:* `#define` or `macro_rules!` indexed as a symbol.


## Relations

### RE01 Calls

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 12 | 0 | 0 | 0 | 5 |

- [x] Rust: `find_calls(top_level)` returns the call to `nested`
- [x] Python: `caller` → `p_top`
- [x] JS/TS: `p_m2` → `p_m`
- [x] Java: `p_use` → `p_m`
- [x] Kotlin: `useIt` calls `mx`
- [x] C#: `Go` → `P_HelperFn`; `P_Use` → `P_M`
- [x] C++: `p_use` → `p_helper`
- [x] Go: `main.go` calls `lib.P_Helper`
- [x] PHP: `p_am` → `__construct`
- [x] Lua: `p_top` → `p_local`, `p_mod`
- [x] Bash: `p_top` → `p_helper`
- [x] PowerShell: `p_top` → `p_helper`
- — HTML: HTML is markup; it has no calls
- — CSS: CSS is markup; it has no calls
- — XML: XML is markup; it has no calls
- — XAML: XAML is markup; it has no calls
- — Markdown: Markdown is markup; it has no calls

*Done when:* Call from one symbol to another, as a `calls` relation.


### RE02 Constructor call

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 5 | 2 | 1 | 0 | 9 |


- — Rust: `Foo::new()` is a plain call (RE01)
- [x] Python: `Child(1)` gives `calls Child`
- [x] JS/TS: `new P_Base(1)` → `calls P_Base`
- [x] Java: `new P_Box(1)` → `calls P_Box`
- [x] Kotlin: `P_Sub()` and `P_Sub(1)` each produce a call relation
- [~] C#: `new Outer()` resolves ambiguously to the class and constructor
- [~] C++: `new P_Sub()` gives no relation; `X()` form not exercised
- [ ] Go: composite literal `T{}` gives no relation
- [x] PHP: `new Box()` produces `calls Box` (MCT probe)
- — Lua: Lua has no constructor call
- — Bash: Bash has no constructor call
- — PowerShell: PowerShell has no constructor call
- — HTML: HTML is markup; it has no constructor call
- — CSS: CSS is markup; it has no constructor call
- — XML: XML is markup; it has no constructor call
- — XAML: XAML is markup; it has no constructor call
- — Markdown: Markdown is markup; it has no constructor call

*Done when:* `new X()` or `X()` gives `calls X`.


### RE03 Super / base constructor call

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 2 | 1 | 4 | 0 | 10 |


- — Rust: Rust has no super / base constructor call
- [x] Python: `super().__init__()` → `calls super`, `calls __init__`
- [~] JS/TS: `super.p_m()` is indexed; `super()` itself produces no call relation
- [ ] Java: `super()` and `this()` give no relation
- [ ] Kotlin: secondary `this()` delegation emits no call relation
- [ ] C#: `: base()` gives no call
- [ ] C++: `P_Sub() : P_Base()` gives no relation
- — Go: Go has no super / base constructor call
- [x] PHP: `parent::__construct()` → `calls __construct`
- — Lua: Lua has no super / base constructor call
- — Bash: Bash has no super / base constructor call
- — PowerShell: PowerShell has no super / base constructor call
- — HTML: HTML is markup; it has no super / base constructor call
- — CSS: CSS is markup; it has no super / base constructor call
- — XML: XML is markup; it has no super / base constructor call
- — XAML: XAML is markup; it has no super / base constructor call
- — Markdown: Markdown is markup; it has no super / base constructor call

*Done when:* `super()`, `parent::__construct()` or a base initializer gives a call to the base constructor.


### RE04 Imports

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 11 | 1 | 0 | 0 | 5 |

- [x] Rust: `use lib::p_helper` → `imports`
- [x] Python: `from sibling import imported` → `imports`
- [x] JS/TS: `import` → `imports`
- [x] Java: `import java.util.List` → `imports`
- [~] Kotlin: alias `as mx` also appears as an external import
- [x] C#: `using System;` appears as an import relation (MCT probe)
- [x] C++: `#include` → `imports`
- [x] Go: `import "lib"` → edge
- [x] PHP: `require_once 'lib.php'` → `imports`
- [x] Lua: `require("lib")` → edge
- [x] Bash: `source` → edge
- [x] PowerShell: dot-sourcing → edge
- — HTML: HTML is markup; it has no imports
- — CSS: CSS is markup; it has no imports
- — XML: XML is markup; it has no imports
- — XAML: XAML is markup; it has no imports
- — Markdown: Markdown is markup; it has no imports

*Done when:* Import, include or require gives an `imports` relation for the module and each name.


### RE05 Class extends class

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 6 | 2 | 1 | 0 | 8 |


- — Rust: Rust has no class extends class
- [x] Python: `Child` extends `Base`
- [x] JS/TS: `P_Sub` extends `P_Base`
- [x] Java: `extends P_Base` (ambiguous: same-name file module)
- [x] Kotlin: `P_Sub` extends `P_Base`
- [~] C#: heuristic; ambiguous when a constructor shares the class name
- [~] C++: ambiguous when the constructor shares the class name
- — Go: Go has no class extends class
- [x] PHP: `P_Sub` extends `P_Base`
- [x] Lua: `Child` extends `Base` via `setmetatable` (MCT probe)
- — Bash: Bash has no class extends class
- [ ] PowerShell: PowerShell classes produce no symbols (corpus test)
- — HTML: HTML is markup; it has no class extends class
- — CSS: CSS is markup; it has no class extends class
- — XML: XML is markup; it has no class extends class
- — XAML: XAML is markup; it has no class extends class
- — Markdown: Markdown is markup; it has no class extends class

*Done when:* Base class gives `extends`.


### RE06 Implements interface / trait

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 5 | 1 | 1 | 0 | 10 |

- [x] Rust: `Widget` implements `ParentTrait` and `ChildTrait`
- — Python: no `implements`; abstract bases use `extends`
- [x] JS/TS: `P_Sub` implements `P_Iface`
- [x] Java: `implements P_Iface` (ambiguous: same-name file module)
- [x] Kotlin: `P_Sub` implements `P_Iface`
- [~] C#: heuristic: `IP_Iface` → `implements`
- — C++: a base class is `extends`
- [ ] Go: structural; no relation
- [x] PHP: `implements` for interface and trait
- — Lua: Lua has no implements interface / trait
- — Bash: Bash has no implements interface / trait
- — PowerShell: PowerShell has no implements interface / trait
- — HTML: HTML is markup; it has no implements interface / trait
- — CSS: CSS is markup; it has no implements interface / trait
- — XML: XML is markup; it has no implements interface / trait
- — XAML: XAML is markup; it has no implements interface / trait
- — Markdown: Markdown is markup; it has no implements interface / trait

*Done when:* Interface or trait implementation gives `implements`, owned by the implementing type.


### RE07 Interface extends interface / supertrait

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 3 | 2 | 2 | 0 | 10 |

- [~] Rust: simple, qualified and generic supertrait names produce `extends`; qualification is not preserved
- — Python: Python has no interface extends interface / supertrait
- [x] JS/TS: `P_Iface` extends `P_Ext`
- [ ] Java: `interface P_Iface extends P_Ext2` gives no relation
- [~] Kotlin: `interface P_Iface : P_Ext` emits `implements`
- [x] C#: `IP_Iface2 : IP_Ext2` → `extends`
- — C++: C++ has no interface extends interface / supertrait
- [ ] Go: embedded interface gives no relation
- [x] PHP: `ChildContract` extends `Contract` (MCT probe)
- — Lua: Lua has no interface extends interface / supertrait
- — Bash: Bash has no interface extends interface / supertrait
- — PowerShell: PowerShell has no interface extends interface / supertrait
- — HTML: HTML is markup; it has no interface extends interface / supertrait
- — CSS: CSS is markup; it has no interface extends interface / supertrait
- — XML: XML is markup; it has no interface extends interface / supertrait
- — XAML: XAML is markup; it has no interface extends interface / supertrait
- — Markdown: Markdown is markup; it has no interface extends interface / supertrait

*Done when:* Interface inheritance gives `extends`.


### RE08 Decorator / annotation / attribute

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 2 | 1 | 4 | 0 | 10 |

- [~] Rust: derive and custom attribute names produce references; conditional attributes are not expanded
- [x] Python: `@abc.abstractmethod` and `@login_required` → `references`
- [ ] JS/TS: `@Component` gives no reference
- [ ] Java: `@Override` gives no reference
- [ ] Kotlin: `@Deprecated` gives no reference
- [ ] C#: `[Obsolete]` produces no reference (MCT probe)
- — C++: C++ has no decorator / annotation / attribute
- — Go: Go has no decorator / annotation / attribute
- [x] PHP: `#[Attr]` → `references`
- — Lua: Lua has no decorator / annotation / attribute
- — Bash: Bash has no decorator / annotation / attribute
- — PowerShell: PowerShell has no decorator / annotation / attribute
- — HTML: HTML is markup; it has no decorator / annotation / attribute
- — CSS: CSS is markup; it has no decorator / annotation / attribute
- — XML: XML is markup; it has no decorator / annotation / attribute
- — XAML: XAML is markup; it has no decorator / annotation / attribute
- — Markdown: Markdown is markup; it has no decorator / annotation / attribute

*Done when:* Decorator or attribute gives a `references` relation.


### RE09 Struct / type embedding

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 0 | 1 | 0 | 16 |


- — Rust: Rust has no struct / type embedding
- — Python: Python has no struct / type embedding
- — JS/TS: JS/TS has no struct / type embedding
- — Java: Java has no struct / type embedding
- — Kotlin: Kotlin has no struct / type embedding
- — C#: C# has no struct / type embedding
- — C++: C++ has no struct / type embedding
- [ ] Go: embedded `P_Embedded` gives no relation
- — PHP: PHP has no struct / type embedding
- — Lua: Lua has no struct / type embedding
- — Bash: Bash has no struct / type embedding
- — PowerShell: PowerShell has no struct / type embedding
- — HTML: HTML is markup; it has no struct / type embedding
- — CSS: CSS is markup; it has no struct / type embedding
- — XML: XML is markup; it has no struct / type embedding
- — XAML: XAML is markup; it has no struct / type embedding
- — Markdown: Markdown is markup; it has no struct / type embedding

*Done when:* Embedded field or type gives a relation.


## Search and lookup

### SE01 Find a definition

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 12 | 0 | 0 | 0 | 5 |

- [x] Rust: `find_symbol("top_level")` returns a `function`
- [x] Python: `find_symbol`
- [x] JS/TS: `list_symbols`
- [x] Java: `list_symbols`
- [x] Kotlin: `list_symbols`
- [x] C#: `list_symbols`
- [x] C++: `list_symbols`
- [x] Go: `list_symbols`
- [x] PHP: `list_symbols`
- [x] Lua: `list_symbols`
- [x] Bash: `list_symbols`
- [x] PowerShell: `list_symbols`
- — HTML: HTML is markup; it has no find a definition
- — CSS: CSS is markup; it has no find a definition
- — XML: XML is markup; it has no find a definition
- — XAML: XAML is markup; it has no find a definition
- — Markdown: Markdown is markup; it has no find a definition

*Done when:* `find_symbol`, `list_symbols` and `search_symbols` return the symbol.


### SE02 Callers and callees

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 11 | 1 | 0 | 0 | 5 |

- [~] Rust: local macro invocations produce calls; method-shaped tokens in macro arguments produce references, not calls
- [x] Python: `find_calls`, `find_callers`
- [x] JS/TS: `find_callers p_top`
- [x] Java: `find_calls`
- [x] Kotlin: `useIt` calls `mx`
- [x] C#: `find_callers P_M`
- [x] C++: `find_calls p_use`
- [x] Go: `main.go` calls `P_Helper`
- [x] PHP: `find_calls p_am`
- [x] Lua: `find_calls`
- [x] Bash: `main.sh` calls `p_helper`
- [x] PowerShell: `find_calls p_top`
- — HTML: HTML is markup; it has no callers and callees
- — CSS: CSS is markup; it has no callers and callees
- — XML: XML is markup; it has no callers and callees
- — XAML: XAML is markup; it has no callers and callees
- — Markdown: Markdown is markup; it has no callers and callees

*Done when:* `find_callers` and `find_calls` return the relation, including inside macros.


### SE03 Every use

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 8 | 3 | 1 | 0 | 5 |

- [x] Rust: `find_references("Add")` returns its `use` import
- [x] Python: references to `abstractmethod`
- [x] JS/TS: references through `import`
- [x] Java: references to `p_helper`
- [x] Kotlin: references to `p_helper`
- [~] C#: static class used only as a qualifier gets no reference
- [~] C++: `P_St s;` gives no reference
- [~] Go: struct used only in a composite literal gets no reference
- [x] PHP: references to `p_helper`
- [ ] Lua: reference to table-based `Child` is not indexed (MCT probe)
- [x] Bash: references to `p_helper`
- [x] PowerShell: references to `p_helper`
- — HTML: HTML is markup; it has no every use
- — CSS: CSS is markup; it has no every use
- — XML: XML is markup; it has no every use
- — XAML: XAML is markup; it has no every use
- — Markdown: Markdown is markup; it has no every use

*Done when:* `find_references` returns each use of the symbol.


### SE04 Blast radius and context

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 12 | 0 | 0 | 0 | 5 |

- [x] Rust: `impact_analysis` and `build_context_pack` return callers, references and a likely test
- [x] Python: `build_context_pack` lists the base class
- [x] JS/TS: `build_context_pack`
- [x] Java: `build_context_pack`
- [x] Kotlin: `build_context_pack(outer)` returns the nested callee
- [x] C#: `build_context_pack`
- [x] C++: `build_context_pack(helper)` returns its caller
- [x] Go: `impact_analysis(P_Helper)` returns the cross-file caller and references
- [x] PHP: `build_context_pack`
- [x] Lua: `build_context_pack(method)` returned its caller
- [x] Bash: `build_context_pack(P_Helper)` returned its caller
- [x] PowerShell: `build_context_pack(P_Helper)` returns its caller
- — HTML: HTML is markup; it has no blast radius and context
- — CSS: CSS is markup; it has no blast radius and context
- — XML: XML is markup; it has no blast radius and context
- — XAML: XAML is markup; it has no blast radius and context
- — Markdown: Markdown is markup; it has no blast radius and context

*Done when:* `impact_analysis` and `build_context_pack` include callers, references and tests.


### SE05 Exact text in strings

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 0 | 11 | 0 | 5 |

- [x] Rust: exact phrase query finds the `SEARCH_TEXT` literal
- [ ] Python: exact phrase query does not list the `page.html` literal
- [ ] JS/TS: exact-phrase query finds no literal string in the indexed TSX fixture
- [ ] Java: literal not collected
- [ ] Kotlin: literal not collected
- [ ] C#: literal not collected
- [ ] C++: literal not collected
- [ ] Go: literal not collected
- [ ] PHP: literal not collected
- [ ] Lua: literal not collected
- [ ] Bash: literal not collected
- [ ] PowerShell: literal not collected
- — HTML: HTML is markup; it has no exact text in strings
- — CSS: CSS is markup; it has no exact text in strings
- — XML: XML is markup; it has no exact text in strings
- — XAML: XAML is markup; it has no exact text in strings
- — Markdown: Markdown is markup; it has no exact text in strings

*Done when:* `hybrid_search` with a `"quoted"` query finds literal text.


## Code quality

### QU01 Dead code

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 6 | 6 | 0 | 0 | 5 |

- [~] Rust: field type references and local macro calls are recorded; the analysis remains heuristic and macro expansion is not performed
- [x] Python: unused function and class reported
- [x] JS/TS: unused function and class reported
- [~] Java: only types: `P_UnusedK` reported, `p_unused` method skipped
- [x] Kotlin: top-level function and class reported
- [~] C#: methods skipped; unused class reported
- [~] C++: unused function and class reported; declaration-only case not exercised
- [~] Go: unused struct and function reported; composite-only case not exercised
- [x] PHP: function and class reported
- [~] Lua: free functions are candidates; table methods not exercised
- [x] Bash: unused function reported
- [x] PowerShell: unused function reported
- — HTML: HTML is markup; it has no dead code
- — CSS: CSS is markup; it has no dead code
- — XML: XML is markup; it has no dead code
- — XAML: XAML is markup; it has no dead code
- — Markdown: Markdown is markup; it has no dead code

*Done when:* `find_dead_code` reports unused symbols of the kinds it checks, without false positives.


### QU02 Repeated code

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 3 | 0 | 9 | 0 | 5 |

- [x] Rust: `duplicate_one` and `duplicate_two` form a candidate group at 2 lines / 5 tokens
- [x] Python: `duplicate_one` and `duplicate_two` form a candidate group at 2 lines / 5 tokens
- [ ] JS/TS: duplicate report lists `javascript_typescript` under `unsupported_languages`
- [ ] Java: listed under `unsupported_languages`
- [ ] Kotlin: listed under `unsupported_languages`
- [ ] C#: listed under `unsupported_languages`
- [ ] C++: listed under `unsupported_languages`
- [ ] Go: listed under `unsupported_languages`
- [ ] PHP: listed under `unsupported_languages`
- [x] Lua: listed as supported by `duplicates` (group not exercised)
- [ ] Bash: listed under `unsupported_languages`
- [ ] PowerShell: listed under `unsupported_languages`
- — HTML: HTML is markup; it has no repeated code
- — CSS: CSS is markup; it has no repeated code
- — XML: XML is markup; it has no repeated code
- — XAML: XAML is markup; it has no repeated code
- — Markdown: Markdown is markup; it has no repeated code

*Done when:* `mct-cli duplicates` covers the language (it has a token source).


## Structure

### ST01 Import graph

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 5 | 7 | 0 | 0 | 5 |

- [x] Rust: `src/lib.rs → src/dependency.rs`; external `Add` is listed separately
- [x] Python: `src/audit.py → src/sibling.py`; standard and framework imports are listed as external
- [x] JS/TS: local import edge verified in the earlier probe; framework files outside `.ts`/`.tsx` are not indexed
- [~] Java: local import has resolved and ambiguous duplicate rows; JDK import has external and unresolved duplicate rows
- [~] Kotlin: alias import adds a false external entry
- [~] C#: `using Audit` produces ambiguous imports to two file modules
- [~] C++: `#include "lib.h"` also listed as external
- [x] Go: edge only, no external (2026-11 check)
- [~] PHP: resolved `require_once` path also listed as external
- [x] Lua: edge `main → lib`, no external
- [~] Bash: resolved `source` path also listed as external
- [~] PowerShell: dot-sourced path also listed as external
- — HTML: HTML is markup; it has no import graph
- — CSS: CSS is markup; it has no import graph
- — XML: XML is markup; it has no import graph
- — XAML: XAML is markup; it has no import graph
- — Markdown: Markdown is markup; it has no import graph

*Done when:* `get_dependency_graph` shows local import edges without false external entries.


### ST02 File outline

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 10 | 2 | 0 | 0 | 5 |

- [x] Rust: declarations shown with bodies collapsed; nested function retained
- [x] Python: declarations shown with bodies collapsed; `nested` retained inside `top_fn`
- [x] JS/TS: `get_file_skeleton(src/audit.tsx)` returned declarations with collapsed bodies
- [x] Java: `get_file_skeleton` returned top-level interface/class declarations
- [x] Kotlin: `get_file_skeleton` returned declarations and nested function
- [x] C#: `get_file_skeleton(src/Audit.cs)` returned the top-level outline
- [x] C++: `get_file_skeleton` returned class, alias and function declarations
- [x] Go: braces collapsed
- [x] PHP: `get_file_skeleton` returned class and function declarations
- [~] Lua: declaration line and placeholder; one-line function shown whole
- [x] Bash: `get_file_skeleton(src/audit.sh)` returned assignment and function outline
- [~] PowerShell: declaration line and placeholder
- — HTML: HTML is markup; it has no file outline
- — CSS: CSS is markup; it has no file outline
- — XML: XML is markup; it has no file outline
- — XAML: XAML is markup; it has no file outline
- — Markdown: Markdown is markup; it has no file outline

*Done when:* `get_file_skeleton` shows the file's declarations with bodies collapsed.


### ST03 Declared dependencies

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 4 | 1 | 6 | 0 | 6 |

- [x] Rust: `get_indexing_status` reports `serde 1.0.0` from `Cargo.toml`
- [~] Python: `requirements.txt` counted; `pyproject.toml` not
- [x] JS/TS: `package.json` counted
- [ ] Java: `pom.xml` not counted
- [ ] Kotlin: `build.gradle.kts` is not reported as a dependency manifest
- [ ] C#: `.csproj` `PackageReference` not read
- [ ] C++: `CMakeLists.txt` is not reported as a dependency manifest
- [x] Go: `go.mod` counted
- [x] PHP: `composer.json` `require` and `require-dev` counted
- [ ] Lua: `.rockspec` not counted
- — Bash: Bash scripts have no package manifest
- [ ] PowerShell: `.psd1` module manifest is not reported by status
- — HTML: HTML is markup; it has no declared dependencies
- — CSS: CSS is markup; it has no declared dependencies
- — XML: XML is markup; it has no declared dependencies
- — XAML: XAML is markup; it has no declared dependencies
- — Markdown: Markdown is markup; it has no declared dependencies

*Done when:* `get_indexing_status` reads the language's manifest.


## Markup and style features

Each markup feature lists all 17 languages. Code languages have no markup features: they are marked not applicable.

### MK01 Find a definition

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 5 | 0 | 0 | 0 | 12 |


- — Rust: code language; markup feature does not apply
- — Python: code language; markup feature does not apply
- — JS/TS: code language; markup feature does not apply
- — Java: code language; markup feature does not apply
- — Kotlin: code language; markup feature does not apply
- — C#: code language; markup feature does not apply
- — C++: code language; markup feature does not apply
- — Go: code language; markup feature does not apply
- — PHP: code language; markup feature does not apply
- — Lua: code language; markup feature does not apply
- — Bash: code language; markup feature does not apply
- — PowerShell: code language; markup feature does not apply
- [x] HTML: by `id`
- [x] CSS: by selector
- [x] XML: by `id`, `name` or `Name`
- [x] XAML: by `x:Name`
- [x] Markdown: notes and headings

*Done when:* Element, selector, heading or note is found by its name.


### MK02 Find references

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 4 | 0 | 1 | 0 | 12 |


- — Rust: code language; markup feature does not apply
- — Python: code language; markup feature does not apply
- — JS/TS: code language; markup feature does not apply
- — Java: code language; markup feature does not apply
- — Kotlin: code language; markup feature does not apply
- — C#: code language; markup feature does not apply
- — C++: code language; markup feature does not apply
- — Go: code language; markup feature does not apply
- — PHP: code language; markup feature does not apply
- — Lua: code language; markup feature does not apply
- — Bash: code language; markup feature does not apply
- — PowerShell: code language; markup feature does not apply
- [x] HTML: `#id` and `.class` of an identified element
- [x] CSS: HTML class and ID selectors resolve to their CSS elements (integration tests)
- [ ] XML: XML parser emits no references or other relations (corpus and parse tests)
- [x] XAML: event handler names
- [x] Markdown: wiki links and anchors

*Done when:* `find_references` returns the links to the element.


### MK03 Dead code

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 0 | 5 | 0 | 12 |


- — Rust: code language; markup feature does not apply
- — Python: code language; markup feature does not apply
- — JS/TS: code language; markup feature does not apply
- — Java: code language; markup feature does not apply
- — Kotlin: code language; markup feature does not apply
- — C#: code language; markup feature does not apply
- — C++: code language; markup feature does not apply
- — Go: code language; markup feature does not apply
- — PHP: code language; markup feature does not apply
- — Lua: code language; markup feature does not apply
- — Bash: code language; markup feature does not apply
- — PowerShell: code language; markup feature does not apply
- [ ] HTML: `find_dead_code` reports no candidates for a markup-only project
- [ ] CSS: `find_dead_code` reports no candidates for a markup-only project
- [ ] XML: `find_dead_code` reports no candidates for a markup-only project
- [ ] XAML: `find_dead_code` reports no candidates for a markup-only project
- [ ] Markdown: `find_dead_code` reports no candidates for a markup-only project

*Done when:* `find_dead_code` reports unused elements. Markup is never reported.


### MK04 Repeated code

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 0 | 5 | 0 | 12 |


- — Rust: code language; markup feature does not apply
- — Python: code language; markup feature does not apply
- — JS/TS: code language; markup feature does not apply
- — Java: code language; markup feature does not apply
- — Kotlin: code language; markup feature does not apply
- — C#: code language; markup feature does not apply
- — C++: code language; markup feature does not apply
- — Go: code language; markup feature does not apply
- — PHP: code language; markup feature does not apply
- — Lua: code language; markup feature does not apply
- — Bash: code language; markup feature does not apply
- — PowerShell: code language; markup feature does not apply
- [ ] HTML: markup-only `duplicates` run produced zero candidates
- [ ] CSS: markup-only `duplicates` run produced zero candidates
- [ ] XML: markup-only `duplicates` run produced zero candidates
- [ ] XAML: markup-only `duplicates` run produced zero candidates
- [ ] Markdown: markup-only `duplicates` run produced zero candidates

*Done when:* `mct-cli duplicates` covers the language.


### MK05 Import graph

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 4 | 1 | 0 | 12 |


- — Rust: code language; markup feature does not apply
- — Python: code language; markup feature does not apply
- — JS/TS: code language; markup feature does not apply
- — Java: code language; markup feature does not apply
- — Kotlin: code language; markup feature does not apply
- — C#: code language; markup feature does not apply
- — C++: code language; markup feature does not apply
- — Go: code language; markup feature does not apply
- — PHP: code language; markup feature does not apply
- — Lua: code language; markup feature does not apply
- — Bash: code language; markup feature does not apply
- — PowerShell: code language; markup feature does not apply
- [~] HTML: `<link>` edge to CSS; script source not resolved
- [~] CSS: local `@import` edge, also listed as external
- [ ] XML: parser emits no imports (confirmed by parse tests)
- [~] XAML: relative source resolves; absolute `pack:` source emits no import relation
- [~] Markdown: only `![[embeds]]` are edges

*Done when:* `get_dependency_graph` shows local file edges.


### MK06 File outline

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 5 | 0 | 0 | 0 | 12 |


- — Rust: code language; markup feature does not apply
- — Python: code language; markup feature does not apply
- — JS/TS: code language; markup feature does not apply
- — Java: code language; markup feature does not apply
- — Kotlin: code language; markup feature does not apply
- — C#: code language; markup feature does not apply
- — C++: code language; markup feature does not apply
- — Go: code language; markup feature does not apply
- — PHP: code language; markup feature does not apply
- — Lua: code language; markup feature does not apply
- — Bash: code language; markup feature does not apply
- — PowerShell: code language; markup feature does not apply
- [x] HTML: `get_file_skeleton` returns its element outline
- [x] CSS: `get_file_skeleton` returns its selector outline
- [x] XML: `get_file_skeleton` returns its element outline
- [x] XAML: `get_file_skeleton` returns its element outline
- [x] Markdown: `get_file_skeleton` returns its heading outline

*Done when:* `get_file_skeleton` shows the structure of the file.


### MK07 Exact text in strings

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 0 | 5 | 0 | 12 |


- — Rust: code language; markup feature does not apply
- — Python: code language; markup feature does not apply
- — JS/TS: code language; markup feature does not apply
- — Java: code language; markup feature does not apply
- — Kotlin: code language; markup feature does not apply
- — C#: code language; markup feature does not apply
- — C++: code language; markup feature does not apply
- — Go: code language; markup feature does not apply
- — PHP: code language; markup feature does not apply
- — Lua: code language; markup feature does not apply
- — Bash: code language; markup feature does not apply
- — PowerShell: code language; markup feature does not apply
- [ ] HTML: exact-phrase query does not find HTML text content
- [ ] CSS: exact-phrase query does not find CSS source text
- [ ] XML: exact-phrase query does not find XML text content
- [ ] XAML: exact-phrase query does not find XAML source text
- [ ] Markdown: exact-phrase query does not find Markdown paragraph text

*Done when:* `hybrid_search` with a `"quoted"` query finds literal text.


