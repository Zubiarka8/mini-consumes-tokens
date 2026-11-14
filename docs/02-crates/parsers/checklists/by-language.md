# Parser checklists by language

> **Verification status:** checked on 2026-11-04 with `mct-cli` (debug build, commit `bb5eb03`) against
> probe fixtures. `[?]` means the feature was not exercised in that check; it does not mean the feature is
> missing. The Rust section was re-probed on 2026-11-09 against the current parser. The earlier record is the 2026-10-19 probe at commit `71ec6d0`.

Secondary view. One checklist per language, all 17 languages. Definitions are in [features.md](../features.md).
Same data as [by-feature.md](by-feature.md).

Marks:

- `[x]` done
- `[~]` partial: works only in some forms; the note says which
- `[ ]` not done
- `[?]` not verified

Every feature the language does not have is named under *Not applicable*, with the reason. Those are left out of its counts.

## Summary

| Language | Done | Partial | Not done | Not verified | Not applicable |
|---|--:|--:|--:|--:|--:|
| Rust | 36 | 0 | 0 | 0 | 17 |
| Python | 25 | 5 | 8 | 0 | 14 |
| JS/TS | 27 | 5 | 11 | 0 | 9 |
| Java | 23 | 6 | 8 | 0 | 15 |
| Kotlin | 27 | 7 | 11 | 0 | 7 |
| C# | 25 | 10 | 10 | 0 | 7 |
| C++ | 22 | 8 | 8 | 0 | 14 |
| Go | 15 | 3 | 11 | 0 | 23 |
| PHP | 30 | 3 | 7 | 0 | 12 |
| Lua | 14 | 5 | 5 | 0 | 28 |
| Bash | 12 | 2 | 3 | 0 | 35 |
| PowerShell | 10 | 4 | 13 | 0 | 25 |
| HTML | 3 | 1 | 3 | 0 | 0 |
| CSS | 3 | 1 | 3 | 0 | 0 |
| XML | 2 | 0 | 5 | 0 | 0 |
| XAML | 3 | 1 | 3 | 0 | 0 |
| Markdown | 3 | 1 | 3 | 0 | 0 |

Markup languages use the MK features and their own constructs, at the end.

## Rust (`rust`)

Done 36 · Partial 0 · Not done 0 · Not verified 0 · Not applicable 17

**Functions and methods**

- [x] FN01 Top-level function: `function`
- [x] FN02 Nested function: nested function is indexed with the enclosing function as `parent`
- [x] FN03 Lambda or closure assigned to a name: `let name = |..| ..` closure is a `function` owned by the enclosing function; closures not bound by a plain `let` are not indexed
- [x] FN04 Method: parent is the impl type
- [x] FN06 Destructor / finalizer: `drop` is a `method` owned by `Widget`
- [x] FN07 Operator overload: `add` is a `method` owned by `Widget`
- [x] FN08 Abstract / pure virtual method: required trait method is indexed as `method` in `ChildTrait`
**Types and object-oriented programming**

- [x] TY03 Struct: `struct`
- [x] TY04 Union: `union RawValue` is indexed as a `struct` with its fields
- [x] TY06 Interface members: trait method as `method`
- [x] TY07 Trait / mixin: `trait`
- [x] TY08 Enum: `enum`
- [x] TY09 Enum members: variants `Red` and `Green` are `constant` symbols owned by `Color`
- [x] TY15 Type alias: `type` alias
- [x] TY16 Module / namespace / package: file module plus `mod` blocks
**Fields, constants and variables**

- [x] DA01 Field / instance attribute: `Widget.inner` and `Widget.value` are `field` symbols; tuple fields are named `0`, `1`
- [x] DA03 Constant (module or global): `const` → `constant`
- [x] DA04 Global / module-level variable: `static` and `static mut` are `variable`; `const` stays `constant`
- [x] DA05 Local variable: `local_value` is a `variable` owned by its function, like the bindings of `for`, `if let`, `while let` and `match` arms
- [x] DA06 Parameter: `input` is a `variable` owned by its function
- [x] DA07 Macro / preprocessor define: `macro_rules! audit_macro` is a `function`; there is no macro kind
**Relations**

- [x] RE01 Calls: `find_calls(top_level)` returns the call to `nested`
- [x] RE04 Imports: `find_references("Add")` returns its `use` import
- [x] RE06 Implements interface / trait: `Widget` implements `ParentTrait` and `ChildTrait`
- [x] RE07 Interface extends interface / supertrait: `ChildTrait: ParentTrait` produces an `extends` relation
- [x] RE08 Decorator / annotation / attribute: `#[derive(Clone)]` produces a `references` relation to `Clone`; compiler attributes such as `#[cfg]` are ignored
**Search and lookup**

- [x] SE01 Find a definition: `find_symbol("top_level")` returns a `function`
- [x] SE02 Callers and callees: `audit_macro!` and an imported function called inside a macro (`println!("{}", helper(x))`) are `calls`
- [x] SE03 Every use: references through `use`
- [x] SE04 Blast radius and context: `impact_analysis` and `build_context_pack` return callers, references and a likely test
- [x] SE05 Exact text in strings: exact phrase query finds the `SEARCH_TEXT` literal
**Code quality**

- [x] QU01 Dead code: unused functions and types are reported; `Inner`, macros and associated types of trait impls are not false positives
- [x] QU02 Repeated code: one REAL group
**Structure**

- [x] ST01 Import graph: `src/lib.rs → src/dependency.rs`; external `Add` is listed separately
- [x] ST02 File outline: `get_file_skeleton` returns declarations with bodies collapsed
- [x] ST03 Declared dependencies: `get_indexing_status` reports `serde 1.0.0` from `Cargo.toml`

**Not applicable:**

- — FN05 Constructor: no constructor keyword; `fn new` is a method
- — FN09 Property accessors (get/set): Rust has no property accessors (get/set)
- — FN10 Overloads kept apart: Rust has no overloads kept apart
- — TY01 Class: Rust has no class
- — TY02 Abstract class: Rust has no abstract class
- — TY05 Interface / protocol: traits are TY07
- — TY10 Record / data class: Rust has no record / data class
- — TY11 Nested / inner class: Rust has no nested / inner class
- — TY12 Anonymous class: Rust has no anonymous class
- — TY13 Partial class: Rust has no partial class
- — TY14 Singleton / companion object: Rust has no singleton / companion object
- — DA02 Constructor-promoted field: Rust has no constructor-promoted field
- — RE02 Constructor call: `Foo::new()` is a plain call (RE01)
- — RE03 Super / base constructor call: Rust has no super / base constructor call
- — RE05 Class extends class: Rust has no class extends class
- — RE09 Struct / type embedding: Rust has no struct / type embedding

Markup features (MK) do not apply to code languages.

## Python (`python`)

Done 25 · Partial 5 · Not done 8 · Not verified 0 · Not applicable 14

**Functions and methods**

- [x] FN01 Top-level function: `async def` too
- [~] FN02 Nested function: file skeleton indents `nested`, but parser test confirms `parent` is `None`
- [ ] FN03 Lambda or closure assigned to a name: `find_symbol("assigned_lambda")` returns no symbol
- [x] FN04 Method: parent is the class
- [x] FN05 Constructor: `__init__`
- [x] FN06 Destructor / finalizer: `__del__`
- [x] FN07 Operator overload: `__add__`
- [x] FN08 Abstract / pure virtual method: `@abstractmethod`
- [~] FN09 Property accessors (get/set): getter and setter are two `p_val` rows
**Types and object-oriented programming**

- [x] TY01 Class: `class`
- [x] TY02 Abstract class: abstract flag not stored
- [~] TY08 Enum: `Enum` subclass is a `class`
- [ ] TY09 Enum members: `find_symbol("RED")` returns no symbol
- [~] TY10 Record / data class: `Record` is a `class`; `field` has no symbol
- [x] TY11 Nested / inner class: parent is the outer class
- [x] TY15 Type alias: PEP 695 `type X = …`
- [x] TY16 Module / namespace / package: file `module`
**Fields, constants and variables**

- [ ] DA01 Field / instance attribute: `self.value` is omitted from the field-symbol query
- [ ] DA03 Constant (module or global): `P_CONST` has no symbol
- [ ] DA04 Global / module-level variable: `P_MOD` has no symbol
- [ ] DA05 Local variable: `find_symbol("local_value")` returns no symbol
- [ ] DA06 Parameter: `find_symbol("argument")` returns no symbol
**Relations**

- [x] RE01 Calls: `caller` → `p_top`
- [x] RE02 Constructor call: `Child(1)` gives `calls Child`
- [x] RE03 Super / base constructor call: `super().__init__()` → `calls super`, `calls __init__`
- [x] RE04 Imports: `from sibling import imported` → `imports`
- [x] RE05 Class extends class: `Child` extends `Base`
- [x] RE08 Decorator / annotation / attribute: `@abc.abstractmethod` and `@login_required` → `references`
**Search and lookup**

- [x] SE01 Find a definition: `find_symbol`
- [x] SE02 Callers and callees: `find_calls`, `find_callers`
- [x] SE03 Every use: references to `abstractmethod`
- [x] SE04 Blast radius and context: `build_context_pack` lists the base class
- [ ] SE05 Exact text in strings: exact phrase query does not list the `page.html` literal
**Code quality**

- [x] QU01 Dead code: unused function and class reported
- [x] QU02 Repeated code: `duplicate_one` and `duplicate_two` form a candidate group at 2 lines / 5 tokens
**Structure**

- [x] ST01 Import graph: `src/audit.py → src/sibling.py`; standard and framework imports are listed as external
- [x] ST02 File outline: `get_file_skeleton` shows declarations with bodies collapsed and retains `nested`
- [~] ST03 Declared dependencies: `requirements.txt` counted; `pyproject.toml` not

**Not applicable:**

- — FN10 Overloads kept apart: Python has no overloads kept apart
- — TY03 Struct: Python has no struct
- — TY04 Union: Python has no union
- — TY05 Interface / protocol: `typing.Protocol` is a class (TY01)
- — TY06 Interface members: Python has no interface members
- — TY07 Trait / mixin: Python has no trait / mixin
- — TY12 Anonymous class: Python has no anonymous class
- — TY13 Partial class: Python has no partial class
- — TY14 Singleton / companion object: Python has no singleton / companion object
- — DA02 Constructor-promoted field: Python has no constructor-promoted field
- — DA07 Macro / preprocessor define: Python has no macro / preprocessor define
- — RE06 Implements interface / trait: no `implements`; abstract bases use `extends`
- — RE07 Interface extends interface / supertrait: Python has no interface extends interface / supertrait
- — RE09 Struct / type embedding: Python has no struct / type embedding

Markup features (MK) do not apply to code languages.

## JS/TS (`javascript_typescript`)

Done 27 · Partial 5 · Not done 11 · Not verified 0 · Not applicable 9

**Functions and methods**

- [x] FN01 Top-level function: `function`
- [~] FN02 Nested function: nested function is indexed, but parser test confirms `parent` is `None`
- [x] FN03 Lambda or closure assigned to a name: `const p_arrow = …` → `function`
- [x] FN04 Method: `p_m` as `method`
- [x] FN05 Constructor: `constructor` as `method`
- [ ] FN08 Abstract / pure virtual method: `abstract p_am(): void;` has no symbol
- [~] FN09 Property accessors (get/set): `get` and `set` are two rows
- [~] FN10 Overloads kept apart: overload signatures dropped; implementation kept
**Types and object-oriented programming**

- [x] TY01 Class: `class`
- [x] TY02 Abstract class: `abstract class` → `class`
- [x] TY05 Interface / protocol: `interface`
- [ ] TY06 Interface members: `P_Iface.p_im` has no symbol; only `Impl.p_im` is indexed
- [~] TY07 Trait / mixin: mixin factory and method are indexed, but the method has no mixin owner
- [x] TY08 Enum: `enum`
- [ ] TY09 Enum members: `find_symbol("Red")` returns no symbol
- [x] TY11 Nested / inner class: `Inner` is indexed with `Outer` as parent
- [ ] TY12 Anonymous class: anonymous class has no class symbol; `anonymousMethod` has no owner
- [x] TY15 Type alias: `type` alias
- [x] TY16 Module / namespace / package: file module plus `namespace`
**Fields, constants and variables**

- [x] DA01 Field / instance attribute: `#p_priv` and `static` fields
- [ ] DA02 Constructor-promoted field: `constructor(public p_promo)` has no symbol
- [ ] DA03 Constant (module or global): `const P_CONST` has no symbol
- [ ] DA04 Global / module-level variable: `let p_var` has no symbol
- [ ] DA05 Local variable: `find_symbol("local_value")` returns no symbol
- [ ] DA06 Parameter: `find_symbol("inner")` returns no symbol
**Relations**

- [x] RE01 Calls: `p_m2` → `p_m`
- [x] RE02 Constructor call: `new P_Base(1)` → `calls P_Base`
- [~] RE03 Super / base constructor call: `super.p_m()` is indexed; `super()` itself produces no call relation
- [x] RE04 Imports: `import` → `imports`
- [x] RE05 Class extends class: `P_Sub` extends `P_Base`
- [x] RE06 Implements interface / trait: `P_Sub` implements `P_Iface`
- [x] RE07 Interface extends interface / supertrait: `P_Iface` extends `P_Ext`
- [ ] RE08 Decorator / annotation / attribute: `@Component` gives no reference
**Search and lookup**

- [x] SE01 Find a definition: `list_symbols`
- [x] SE02 Callers and callees: `find_callers p_top`
- [x] SE03 Every use: references through `import`
- [x] SE04 Blast radius and context: `build_context_pack`
- [ ] SE05 Exact text in strings: exact-phrase query finds no literal string in the indexed TSX fixture
**Code quality**

- [x] QU01 Dead code: unused function and class reported
- [x] QU02 Repeated code: `cartTotal` and `orderTotal` form a candidate group; listed as supported by `duplicates`
**Structure**

- [x] ST01 Import graph: edge `main.js → lib.js`, no external
- [x] ST02 File outline: `get_file_skeleton(src/audit.tsx)` returned declarations with collapsed bodies
- [x] ST03 Declared dependencies: `package.json` counted

**Not applicable:**

- — FN06 Destructor / finalizer: JS/TS has no destructor / finalizer
- — FN07 Operator overload: JS/TS has no operator overload
- — TY03 Struct: JS/TS has no struct
- — TY04 Union: JS/TS has no union
- — TY10 Record / data class: JS/TS has no record / data class
- — TY13 Partial class: JS/TS has no partial class
- — TY14 Singleton / companion object: JS/TS has no singleton / companion object
- — DA07 Macro / preprocessor define: JS/TS has no macro / preprocessor define
- — RE09 Struct / type embedding: JS/TS has no struct / type embedding

Markup features (MK) do not apply to code languages.

## Java (`java`)

Done 23 · Partial 6 · Not done 8 · Not verified 0 · Not applicable 15

**Functions and methods**

- [ ] FN03 Lambda or closure assigned to a name: lambda `r` has no symbol
- [x] FN04 Method: `method`
- [x] FN05 Constructor: constructor as `method`
- [x] FN08 Abstract / pure virtual method: abstract `p_am`
- [x] FN10 Overloads kept apart: two `p_m` rows
**Types and object-oriented programming**

- [x] TY01 Class: `class`
- [x] TY02 Abstract class: `abstract static class` → `class`
- [x] TY05 Interface / protocol: `interface`
- [x] TY06 Interface members: `p_im` in interface
- [x] TY08 Enum: `enum`
- [x] TY09 Enum members: enum constants as `field`
- [~] TY10 Record / data class: `record` is a `class`; `p_comp` has no symbol
- [x] TY11 Nested / inner class: static and inner
- [~] TY12 Anonymous class: method inside a method → enclosing class; field initializer → no symbol
- [~] TY16 Module / namespace / package: `package` has no symbol
**Fields, constants and variables**

- [x] DA01 Field / instance attribute: `field`
- [~] DA03 Constant (module or global): `static final` is a `field`
- [ ] DA05 Local variable: lambda local not indexed
- [ ] DA06 Parameter: parameter not indexed
**Relations**

- [x] RE01 Calls: `p_use` → `p_m`
- [x] RE02 Constructor call: `new P_Box(1)` → `calls P_Box`
- [ ] RE03 Super / base constructor call: `super()` and `this()` give no relation
- [x] RE04 Imports: `import java.util.List` → `imports`
- [x] RE05 Class extends class: `extends P_Base` (ambiguous: same-name file module)
- [x] RE06 Implements interface / trait: `implements P_Iface` (ambiguous: same-name file module)
- [ ] RE07 Interface extends interface / supertrait: `interface P_Iface extends P_Ext2` gives no relation
- [ ] RE08 Decorator / annotation / attribute: `@Override` gives no reference
**Search and lookup**

- [x] SE01 Find a definition: `list_symbols`
- [x] SE02 Callers and callees: `find_calls`
- [x] SE03 Every use: references to `p_helper`
- [x] SE04 Blast radius and context: `build_context_pack`
- [ ] SE05 Exact text in strings: literal not collected
**Code quality**

- [~] QU01 Dead code: only types: `P_UnusedK` reported, `p_unused` method skipped
- [x] QU02 Repeated code: `cartTotal` and `orderTotal` form a candidate group; listed as supported by `duplicates`
**Structure**

- [~] ST01 Import graph: local imports resolve, but each import duplicates resolved/ambiguous rows; JDK import duplicates external/unresolved rows
- [x] ST02 File outline: `get_file_skeleton` returned the top-level interface/class outline
- [ ] ST03 Declared dependencies: `pom.xml` not counted

**Not applicable:**

- — FN01 Top-level function: no free functions
- — FN02 Nested function: no nested named functions
- — FN06 Destructor / finalizer: Java has no destructor / finalizer
- — FN07 Operator overload: Java has no operator overload
- — FN09 Property accessors (get/set): getters are ordinary methods
- — TY03 Struct: Java has no struct
- — TY04 Union: Java has no union
- — TY07 Trait / mixin: Java has no trait / mixin
- — TY13 Partial class: Java has no partial class
- — TY14 Singleton / companion object: Java has no singleton / companion object
- — TY15 Type alias: Java has no type alias
- — DA02 Constructor-promoted field: Java has no constructor-promoted field
- — DA04 Global / module-level variable: Java has no global / module-level variable
- — DA07 Macro / preprocessor define: Java has no macro / preprocessor define
- — RE09 Struct / type embedding: Java has no struct / type embedding

Markup features (MK) do not apply to code languages.

## Kotlin (`kotlin`)

Done 27 · Partial 7 · Not done 11 · Not verified 0 · Not applicable 7

**Functions and methods**

- [x] FN01 Top-level function: `function`
- [~] FN02 Nested function: `inner` is indexed as a function with no parent association
- [ ] FN03 Lambda or closure assigned to a name: `val f = { … }` has no symbol
- [x] FN04 Method: extension `String.p_ext` → `method`
- [ ] FN05 Constructor: secondary constructor has no symbol
- [x] FN07 Operator overload: `operator fun plus`
- [x] FN08 Abstract / pure virtual method: `abstract fun p_am`
- [x] FN09 Property accessors (get/set): one `field`
- [x] FN10 Overloads kept apart: two `p_ov` rows
**Types and object-oriented programming**

- [x] TY01 Class: `class`
- [x] TY02 Abstract class: `abstract class` → `class`
- [x] TY05 Interface / protocol: `interface`
- [x] TY06 Interface members: `fun p_im()` in interface
- [~] TY08 Enum: `enum class` is a `class`
- [x] TY09 Enum members: entries as `field`
- [x] TY10 Record / data class: `data class` → `class`; `val` parameter → `field`
- [x] TY11 Nested / inner class: `P_Nested`, `P_In`
- [~] TY12 Anonymous class: `object : P_Abs()` gives no class; its function has no parent
- [~] TY14 Singleton / companion object: `companion object` has no symbol; `P_COMP` is a `field` of the outer class
- [ ] TY15 Type alias: `typealias` has no symbol
- [x] TY16 Module / namespace / package: `package` module
**Fields, constants and variables**

- [x] DA01 Field / instance attribute: `var p_prop`
- [x] DA02 Constructor-promoted field: `val p_promo` → `field`
- [ ] DA03 Constant (module or global): top-level `const val` has no symbol
- [ ] DA04 Global / module-level variable: top-level `val` has no symbol
- [ ] DA05 Local variable: `find_symbol(local)` returns no symbol (MCT probe)
- [ ] DA06 Parameter: `find_symbol(input)` returns no symbol (MCT probe)
**Relations**

- [x] RE01 Calls: `useIt` calls `mx`
- [x] RE02 Constructor call: both `P_Sub()` and `P_Sub(1)` are calls
- [ ] RE03 Super / base constructor call: secondary `this()` delegation emits no call relation
- [~] RE04 Imports: alias `as mx` also appears as an external import
- [x] RE05 Class extends class: `P_Sub` extends `P_Base`
- [x] RE06 Implements interface / trait: `P_Sub` implements `P_Iface`
- [~] RE07 Interface extends interface / supertrait: `interface P_Iface : P_Ext` emits `implements`
- [ ] RE08 Decorator / annotation / attribute: `@Deprecated` gives no reference
**Search and lookup**

- [x] SE01 Find a definition: `list_symbols`
- [x] SE02 Callers and callees: `useIt` calls `mx`
- [x] SE03 Every use: references to `p_helper`
- [x] SE04 Blast radius and context: `build_context_pack(outer)` returned the nested callee
- [ ] SE05 Exact text in strings: literal not collected
**Code quality**

- [x] QU01 Dead code: top-level function and class reported
- [x] QU02 Repeated code: `cartTotal` and `orderTotal` form a candidate group; listed as supported by `duplicates`
**Structure**

- [~] ST01 Import graph: alias import adds a false external entry
- [x] ST02 File outline: `get_file_skeleton` returned top-level declarations and nested function
- [ ] ST03 Declared dependencies: `build.gradle.kts` is not detected in status

**Not applicable:**

- — FN06 Destructor / finalizer: Kotlin has no destructor / finalizer
- — TY03 Struct: Kotlin has no struct
- — TY04 Union: Kotlin has no union
- — TY07 Trait / mixin: Kotlin has no trait / mixin
- — TY13 Partial class: Kotlin has no partial class
- — DA07 Macro / preprocessor define: Kotlin has no macro / preprocessor define
- — RE09 Struct / type embedding: Kotlin has no struct / type embedding

Markup features (MK) do not apply to code languages.

## C# (`csharp`)

Done 25 · Partial 10 · Not done 10 · Not verified 0 · Not applicable 7

**Functions and methods**

- [~] FN02 Nested function: local function is indexed with `parent=None` (MCT probe)
- [ ] FN03 Lambda or closure assigned to a name: `Func<int,int> f` has no symbol (MCT probe)
- [x] FN04 Method: methods as `method`
- [x] FN05 Constructor: constructor `P_Base`
- [x] FN06 Destructor / finalizer: `~P_Base`
- [x] FN07 Operator overload: `operator +`
- [x] FN08 Abstract / pure virtual method: `abstract P_Am`
- [x] FN09 Property accessors (get/set): property `P_Prop` as `field`
- [x] FN10 Overloads kept apart: two `P_Ov` rows
**Types and object-oriented programming**

- [x] TY01 Class: `class`
- [x] TY02 Abstract class: abstract class → `class`
- [x] TY03 Struct: `struct`
- [x] TY05 Interface / protocol: `interface`
- [x] TY06 Interface members: methods as `method`
- [x] TY08 Enum: `enum`
- [x] TY09 Enum members: members as `constant`
- [~] TY10 Record / data class: `record` is a `class`; `P_Comp` has no symbol
- [x] TY11 Nested / inner class: parent is the outer class
- [ ] TY12 Anonymous class: anonymous type `new { X = 1 }` has no symbol
- [x] TY13 Partial class: one row per declaration
- [~] TY15 Type alias: `delegate` → `type_alias`; `using` alias emits imports
- [x] TY16 Module / namespace / package: `namespace P.Ns` → `module`
**Fields, constants and variables**

- [x] DA01 Field / instance attribute: fields, properties and events as `field`
- [ ] DA02 Constructor-promoted field: record positional parameter has no symbol
- [~] DA03 Constant (module or global): `const` is a `field`
- [ ] DA05 Local variable: `find_symbol(local)` returns no symbol (MCT probe)
- [ ] DA06 Parameter: `find_symbol(input)` returns no symbol (MCT probe)
**Relations**

- [x] RE01 Calls: `Go` → `P_HelperFn`; `P_Use` → `P_M`
- [~] RE02 Constructor call: `new Outer()` is indexed but resolves ambiguously to the class and constructor
- [ ] RE03 Super / base constructor call: `: base()` gives no call
- [x] RE04 Imports: `using System;` parsed as an import relation (MCT probe)
- [~] RE05 Class extends class: heuristic; ambiguous when a constructor shares the class name
- [~] RE06 Implements interface / trait: heuristic: `IP_Iface` → `implements`
- [x] RE07 Interface extends interface / supertrait: `IP_Iface2 : IP_Ext2` → `extends`
- [ ] RE08 Decorator / annotation / attribute: `[Obsolete]` produces no reference (MCT probe)
**Search and lookup**

- [x] SE01 Find a definition: `list_symbols`
- [x] SE02 Callers and callees: `find_callers P_M`
- [~] SE03 Every use: static class used only as a qualifier gets no reference
- [x] SE04 Blast radius and context: `build_context_pack`
- [ ] SE05 Exact text in strings: literal not collected
**Code quality**

- [~] QU01 Dead code: methods skipped; unused class reported
- [ ] QU02 Repeated code: listed under `unsupported_languages`
**Structure**

- [~] ST01 Import graph: `using Audit` is recorded as an ambiguous import to two file modules
- [x] ST02 File outline: `get_file_skeleton(src/Audit.cs)` returned the top-level outline
- [ ] ST03 Declared dependencies: `.csproj` `PackageReference` not read

**Not applicable:**

- — FN01 Top-level function: methods only; local functions are FN02
- — TY04 Union: C# has no union
- — TY07 Trait / mixin: C# has no trait / mixin
- — TY14 Singleton / companion object: C# has no singleton / companion object
- — DA04 Global / module-level variable: C# has no global / module-level variable
- — DA07 Macro / preprocessor define: C# has no macro / preprocessor define
- — RE09 Struct / type embedding: C# has no struct / type embedding

Markup features (MK) do not apply to code languages.

## C++ (`cpp`)

Done 22 · Partial 8 · Not done 8 · Not verified 0 · Not applicable 14

**Functions and methods**

- [x] FN01 Top-level function: `p_helper`; namespace functions
- [ ] FN03 Lambda or closure assigned to a name: lambda `l` has no symbol
- [x] FN04 Method: out-of-line member has its class as parent
- [x] FN05 Constructor: constructor `P_Base` as `method`
- [x] FN06 Destructor / finalizer: `~P_Base`
- [x] FN07 Operator overload: `operator+`
- [x] FN08 Abstract / pure virtual method: `= 0` method
- [x] FN10 Overloads kept apart: both `overloaded` definitions are retained (MCT probe)
**Types and object-oriented programming**

- [x] TY01 Class: `class`
- [~] TY02 Abstract class: pure-virtual `Base` is indexed as `struct`; abstractness is not represented
- [x] TY03 Struct: `struct`
- [~] TY04 Union: `P_U` indexed as `struct`
- [x] TY08 Enum: `enum` and `enum class`
- [x] TY09 Enum members: values as `constant`
- [x] TY11 Nested / inner class: nested `P_Nested`
- [x] TY15 Type alias: `using` and `typedef`
- [x] TY16 Module / namespace / package: `namespace pns` → `module`
**Fields, constants and variables**

- [x] DA01 Field / instance attribute: `p_field` and union members
- [~] DA03 Constant (module or global): `const int P_CONST` → `variable`
- [x] DA04 Global / module-level variable: `p_global` → `variable`
- [ ] DA05 Local variable: `auto l` not indexed
- [ ] DA06 Parameter: `find_symbol(input)` returns no symbol (MCT probe)
- [ ] DA07 Macro / preprocessor define: `#define P_MACRO` has no symbol
**Relations**

- [x] RE01 Calls: `p_use` → `p_helper`
- [~] RE02 Constructor call: `new P_Sub()` gives no relation; `X()` form not exercised
- [ ] RE03 Super / base constructor call: `P_Sub() : P_Base()` gives no relation
- [x] RE04 Imports: `#include` → `imports`
- [~] RE05 Class extends class: ambiguous when the constructor shares the class name
**Search and lookup**

- [x] SE01 Find a definition: `list_symbols`
- [x] SE02 Callers and callees: `find_calls p_use`
- [~] SE03 Every use: `P_St s;` gives no reference
- [x] SE04 Blast radius and context: `build_context_pack(helper)` returned its caller
- [ ] SE05 Exact text in strings: literal not collected
**Code quality**

- [~] QU01 Dead code: unused function and class reported; declaration-only case not exercised
- [ ] QU02 Repeated code: listed under `unsupported_languages`
**Structure**

- [~] ST01 Import graph: `#include "lib.h"` also listed as external
- [x] ST02 File outline: `get_file_skeleton(src/audit.cpp)` returned top-level declarations
- [ ] ST03 Declared dependencies: `CMakeLists.txt` is not reported by status

**Not applicable:**

- — FN02 Nested function: C++ has no nested named functions
- — FN09 Property accessors (get/set): C++ has no property accessors (get/set)
- — TY05 Interface / protocol: C++ has no interface / protocol
- — TY06 Interface members: C++ has no interface members
- — TY07 Trait / mixin: C++ has no trait / mixin
- — TY10 Record / data class: C++ has no record / data class
- — TY12 Anonymous class: C++ has no anonymous class
- — TY13 Partial class: C++ has no partial class
- — TY14 Singleton / companion object: C++ has no singleton / companion object
- — DA02 Constructor-promoted field: C++ has no constructor-promoted field
- — RE06 Implements interface / trait: a base class is `extends`
- — RE07 Interface extends interface / supertrait: C++ has no interface extends interface / supertrait
- — RE08 Decorator / annotation / attribute: C++ has no decorator / annotation / attribute
- — RE09 Struct / type embedding: C++ has no struct / type embedding

Markup features (MK) do not apply to code languages.

## Go (`go`)

Done 15 · Partial 3 · Not done 11 · Not verified 0 · Not applicable 23

**Functions and methods**

- [x] FN01 Top-level function: parent is the package
- [ ] FN03 Lambda or closure assigned to a name: `f := func()` has no symbol
- [x] FN04 Method: pointer receiver as `method`
**Types and object-oriented programming**

- [x] TY03 Struct: `struct`
- [x] TY05 Interface / protocol: `interface`
- [x] TY06 Interface members: `P_Im` in interface
- [x] TY15 Type alias: both alias forms
- [~] TY16 Module / namespace / package: package `module` and file module; same name only when the file is named after the package
**Fields, constants and variables**

- [ ] DA01 Field / instance attribute: struct fields have no symbol
- [ ] DA03 Constant (module or global): `const` has no symbol
- [ ] DA04 Global / module-level variable: `var` has no symbol
- [ ] DA05 Local variable: `find_symbol(local)` returns no symbol (MCT probe)
- [ ] DA06 Parameter: `find_symbol(input)` returns no symbol (MCT probe)
**Relations**

- [x] RE01 Calls: `main.go` calls `lib.P_Helper`
- [ ] RE02 Constructor call: composite literal `T{}` gives no relation
- [x] RE04 Imports: `import "lib"` → edge
- [ ] RE06 Implements interface / trait: structural; no relation
- [ ] RE07 Interface extends interface / supertrait: embedded interface gives no relation
- [ ] RE09 Struct / type embedding: embedded `P_Embedded` gives no relation
**Search and lookup**

- [x] SE01 Find a definition: `list_symbols`
- [x] SE02 Callers and callees: `main.go` calls `P_Helper`
- [~] SE03 Every use: struct used only in a composite literal gets no reference
- [x] SE04 Blast radius and context: `build_context_pack(P_Helper)` returned the cross-file caller
- [ ] SE05 Exact text in strings: literal not collected
**Code quality**

- [~] QU01 Dead code: unused struct and function reported; composite-only case not exercised
- [x] QU02 Repeated code: `CartTotal` and `OrderTotal` form a candidate group; listed as supported by `duplicates`
**Structure**

- [x] ST01 Import graph: edge only, no external (2026-11 check)
- [x] ST02 File outline: braces collapsed
- [x] ST03 Declared dependencies: `go.mod` counted

**Not applicable:**

- — FN02 Nested function: Go has no nested function
- — FN05 Constructor: no constructor keyword; `NewX` is a function
- — FN06 Destructor / finalizer: Go has no destructor / finalizer
- — FN07 Operator overload: Go has no operator overload
- — FN08 Abstract / pure virtual method: Go has no abstract / pure virtual method
- — FN09 Property accessors (get/set): Go has no property accessors (get/set)
- — FN10 Overloads kept apart: Go has no overloads kept apart
- — TY01 Class: Go has no class
- — TY02 Abstract class: Go has no abstract class
- — TY04 Union: Go has no union
- — TY07 Trait / mixin: Go has no trait / mixin
- — TY08 Enum: Go has no enum
- — TY09 Enum members: Go has no enum members
- — TY10 Record / data class: Go has no record / data class
- — TY11 Nested / inner class: Go has no nested / inner class
- — TY12 Anonymous class: Go has no anonymous class
- — TY13 Partial class: Go has no partial class
- — TY14 Singleton / companion object: Go has no singleton / companion object
- — DA02 Constructor-promoted field: Go has no constructor-promoted field
- — DA07 Macro / preprocessor define: Go has no macro / preprocessor define
- — RE03 Super / base constructor call: Go has no super / base constructor call
- — RE05 Class extends class: Go has no class extends class
- — RE08 Decorator / annotation / attribute: Go has no decorator / annotation / attribute

Markup features (MK) do not apply to code languages.

## PHP (`php`)

Done 30 · Partial 3 · Not done 7 · Not verified 0 · Not applicable 12

**Functions and methods**

- [x] FN01 Top-level function: `function`
- [~] FN02 Nested function: named `nestedNamed` is indexed with no parent association
- [x] FN03 Lambda or closure assigned to a name: `$f = function` → `function`
- [x] FN04 Method: `method`
- [x] FN05 Constructor: `__construct`
- [x] FN06 Destructor / finalizer: `__destruct`
- [x] FN08 Abstract / pure virtual method: abstract `p_am`
**Types and object-oriented programming**

- [x] TY01 Class: `class`
- [x] TY02 Abstract class: `abstract class` → `class`
- [x] TY05 Interface / protocol: `interface`
- [x] TY06 Interface members: interface method
- [x] TY07 Trait / mixin: `trait` with its methods
- [x] TY08 Enum: `enum`
- [x] TY09 Enum members: `case` as `field`
- — TY11 Nested / inner class: PHP has no nested class syntax; the [nested-class RFC was declined](https://wiki.php.net/rfc/short-and-inner-classes)
- [ ] TY12 Anonymous class: `new class {}` gives no symbols
- [~] TY16 Module / namespace / package: `namespace` is not a symbol
**Fields, constants and variables**

- [x] DA01 Field / instance attribute: property `$prop`
- [x] DA02 Constructor-promoted field: promoted parameter → `field`
- [x] DA03 Constant (module or global): `const` top-level and in classes
- [ ] DA04 Global / module-level variable: `$p_glob = …` has no symbol
- [ ] DA05 Local variable: `find_symbol(local)` returns no symbol (MCT probe)
- [ ] DA06 Parameter: `find_symbol(input)` returns no symbol (MCT probe)
**Relations**

- [x] RE01 Calls: `p_am` → `__construct`
- [x] RE02 Constructor call: `new Box()` produces `calls Box` (MCT probe)
- [x] RE03 Super / base constructor call: `parent::__construct()` → `calls __construct`
- [x] RE04 Imports: `require_once 'lib.php'` → `imports`
- [x] RE05 Class extends class: `P_Sub` extends `P_Base`
- [x] RE06 Implements interface / trait: `implements` for interface and trait
- [x] RE07 Interface extends interface / supertrait: `ChildContract` extends `Contract` (MCT probe)
- [x] RE08 Decorator / annotation / attribute: `#[Attr]` → `references`
**Search and lookup**

- [x] SE01 Find a definition: `list_symbols`
- [x] SE02 Callers and callees: `find_calls p_am`
- [x] SE03 Every use: references to `p_helper`
- [x] SE04 Blast radius and context: `build_context_pack`
- [ ] SE05 Exact text in strings: literal not collected
**Code quality**

- [x] QU01 Dead code: function and class reported
- [ ] QU02 Repeated code: listed under `unsupported_languages`
**Structure**

- [~] ST01 Import graph: resolved `require_once` path also listed as external
- [x] ST02 File outline: `get_file_skeleton(src/audit.php)` returned declarations and collapsed bodies
- [ ] ST03 Declared dependencies: `composer.json` not read

**Not applicable:**

- — FN07 Operator overload: PHP has no operator overload
- — FN09 Property accessors (get/set): PHP has no property accessors (get/set)
- — FN10 Overloads kept apart: PHP has no overloads kept apart
- — TY03 Struct: PHP has no struct
- — TY04 Union: PHP has no union
- — TY10 Record / data class: PHP has no record / data class
- — TY13 Partial class: PHP has no partial class
- — TY14 Singleton / companion object: PHP has no singleton / companion object
- — TY15 Type alias: PHP has no type alias
- — DA07 Macro / preprocessor define: `define()` stays a call by decision
- — RE09 Struct / type embedding: PHP has no struct / type embedding

Markup features (MK) do not apply to code languages.

## Lua (`lua`)

Done 14 · Partial 5 · Not done 5 · Not verified 0 · Not applicable 28

**Functions and methods**

- [x] FN01 Top-level function: `function`
- [~] FN02 Nested function: `nested` is indexed and callable, but has no parent association
- [x] FN03 Lambda or closure assigned to a name: `local p_lam = function`
- [x] FN04 Method: `M.p_mod` and `M:p_meth` as `method`
- [~] FN07 Operator overload: `__add` is indexed as a method; no operator relation is emitted
**Types and object-oriented programming**

- [x] TY01 Class: `local Widget = setmetatable({}, …)` → `class` (2026-11 record; the 2026-10 record said no symbol)
- [x] TY16 Module / namespace / package: file `module` and table `module`
**Fields, constants and variables**

- [ ] DA01 Field / instance attribute: `self.x` produces no field symbol (MCT probe)
- [~] DA04 Global / module-level variable: global `MSG = …` → `variable`; top-level `local` not exercised
- [x] DA05 Local variable: `local_value` is indexed under `method`
- [ ] DA06 Parameter: `find_symbol(input)` returns no symbol (MCT probe)
**Relations**

- [x] RE01 Calls: `p_top` → `p_local`, `p_mod`
- [x] RE04 Imports: `require("lib")` → edge
- [x] RE05 Class extends class: `Child` extends `Base` via `setmetatable` (MCT probe)
**Search and lookup**

- [x] SE01 Find a definition: `list_symbols`
- [x] SE02 Callers and callees: `find_calls`
- [ ] SE03 Every use: reference to table-based `Child` is not indexed (MCT probe)
- [x] SE04 Blast radius and context: `build_context_pack(method)` returned its caller
- [ ] SE05 Exact text in strings: literal not collected
**Code quality**

- [~] QU01 Dead code: free functions are candidates; table methods not exercised
- [x] QU02 Repeated code: listed as supported by `duplicates` (group not exercised)
**Structure**

- [x] ST01 Import graph: edge `main → lib`, no external
- [~] ST02 File outline: declaration line and placeholder; one-line function shown whole
- [ ] ST03 Declared dependencies: `.rockspec` not counted

**Not applicable:**

- — FN05 Constructor: no constructor keyword
- — FN06 Destructor / finalizer: Lua has no destructor / finalizer
- — FN08 Abstract / pure virtual method: Lua has no abstract / pure virtual method
- — FN09 Property accessors (get/set): Lua has no property accessors (get/set)
- — FN10 Overloads kept apart: Lua has no overloads kept apart
- — TY02 Abstract class: Lua has no abstract class
- — TY03 Struct: Lua has no struct
- — TY04 Union: Lua has no union
- — TY05 Interface / protocol: Lua has no interface / protocol
- — TY06 Interface members: Lua has no interface members
- — TY07 Trait / mixin: Lua has no trait / mixin
- — TY08 Enum: Lua has no enum
- — TY09 Enum members: Lua has no enum members
- — TY10 Record / data class: Lua has no record / data class
- — TY11 Nested / inner class: Lua has no nested / inner class
- — TY12 Anonymous class: Lua has no anonymous class
- — TY13 Partial class: Lua has no partial class
- — TY14 Singleton / companion object: Lua has no singleton / companion object
- — TY15 Type alias: Lua has no type alias
- — DA02 Constructor-promoted field: Lua has no constructor-promoted field
- — DA03 Constant (module or global): no `const` keyword
- — DA07 Macro / preprocessor define: Lua has no macro / preprocessor define
- — RE02 Constructor call: Lua has no constructor call
- — RE03 Super / base constructor call: Lua has no super / base constructor call
- — RE06 Implements interface / trait: Lua has no implements interface / trait
- — RE07 Interface extends interface / supertrait: Lua has no interface extends interface / supertrait
- — RE08 Decorator / annotation / attribute: Lua has no decorator / annotation / attribute
- — RE09 Struct / type embedding: Lua has no struct / type embedding

Markup features (MK) do not apply to code languages.

## Bash (`bash`)

Done 12 · Partial 2 · Not done 3 · Not verified 0 · Not applicable 35

**Functions and methods**

- [x] FN01 Top-level function: `f() {}`
- [x] FN02 Nested function: parent is the outer function
**Types and object-oriented programming**

- [x] TY16 Module / namespace / package: file `module`
**Fields, constants and variables**

- [~] DA03 Constant (module or global): `readonly X=…` → `variable`
- [x] DA04 Global / module-level variable: `P_GLOBAL` → `variable`
- [ ] DA05 Local variable: `local` is not indexed (by design)
**Relations**

- [x] RE01 Calls: `p_top` → `p_helper`
- [x] RE04 Imports: `source` → edge
**Search and lookup**

- [x] SE01 Find a definition: `list_symbols`
- [x] SE02 Callers and callees: `main.sh` calls `p_helper`
- [x] SE03 Every use: references to `p_helper`
- [x] SE04 Blast radius and context: `build_context_pack(P_Helper)` returned its caller
- [ ] SE05 Exact text in strings: literal not collected
**Code quality**

- [x] QU01 Dead code: unused function reported
- [ ] QU02 Repeated code: listed under `unsupported_languages`
**Structure**

- [~] ST01 Import graph: resolved `source` path also listed as external
- [x] ST02 File outline: `get_file_skeleton(src/audit.sh)` returned assignments and functions
- — ST03 Declared dependencies: Bash scripts have no package manifest

**Not applicable:**

- — FN03 Lambda or closure assigned to a name: Bash has no lambda or closure assigned to a name
- — FN04 Method: Bash has no method
- — FN05 Constructor: Bash has no constructor
- — FN06 Destructor / finalizer: Bash has no destructor / finalizer
- — FN07 Operator overload: Bash has no operator overload
- — FN08 Abstract / pure virtual method: Bash has no abstract / pure virtual method
- — FN09 Property accessors (get/set): Bash has no property accessors (get/set)
- — FN10 Overloads kept apart: Bash has no overloads kept apart
- — TY01 Class: Bash has no class
- — TY02 Abstract class: Bash has no abstract class
- — TY03 Struct: Bash has no struct
- — TY04 Union: Bash has no union
- — TY05 Interface / protocol: Bash has no interface / protocol
- — TY06 Interface members: Bash has no interface members
- — TY07 Trait / mixin: Bash has no trait / mixin
- — TY08 Enum: Bash has no enum
- — TY09 Enum members: Bash has no enum members
- — TY10 Record / data class: Bash has no record / data class
- — TY11 Nested / inner class: Bash has no nested / inner class
- — TY12 Anonymous class: Bash has no anonymous class
- — TY13 Partial class: Bash has no partial class
- — TY14 Singleton / companion object: Bash has no singleton / companion object
- — TY15 Type alias: Bash has no type alias
- — DA01 Field / instance attribute: Bash has no field / instance attribute
- — DA02 Constructor-promoted field: Bash has no constructor-promoted field
- — DA06 Parameter: Bash has no parameter
- — DA07 Macro / preprocessor define: Bash has no macro / preprocessor define
- — RE02 Constructor call: Bash has no constructor call
- — RE03 Super / base constructor call: Bash has no super / base constructor call
- — RE05 Class extends class: Bash has no class extends class
- — RE06 Implements interface / trait: Bash has no implements interface / trait
- — RE07 Interface extends interface / supertrait: Bash has no interface extends interface / supertrait
- — RE08 Decorator / annotation / attribute: Bash has no decorator / annotation / attribute
- — RE09 Struct / type embedding: Bash has no struct / type embedding

Markup features (MK) do not apply to code languages.

## PowerShell (`powershell`)

Done 10 · Partial 4 · Not done 13 · Not verified 0 · Not applicable 25

**Functions and methods**

- [~] FN01 Top-level function: multi-line works; one-line `function f { param(...) … }` is a syntax error
- [x] FN02 Nested function: parent is the outer function
- [~] FN03 Lambda or closure assigned to a name: top-level `$f = { … }` is a `variable`
- [ ] FN04 Method: PowerShell classes are not indexed (corpus test)
- [ ] FN05 Constructor: PowerShell classes are not indexed (corpus test)
- [ ] FN09 Property accessors (get/set): PowerShell classes are not indexed (corpus test)
**Types and object-oriented programming**

- [ ] TY01 Class: corpus test confirms PowerShell classes produce no symbol
- [ ] TY08 Enum: corpus test confirms PowerShell enums produce no symbol
- [ ] TY09 Enum members: no enum symbol means its members are not indexed
- [x] TY16 Module / namespace / package: file `module`
**Fields, constants and variables**

- [ ] DA01 Field / instance attribute: PowerShell class properties are not indexed (corpus test)
- [x] DA04 Global / module-level variable: scope prefixes stripped
- [ ] DA05 Local variable: `find_symbol(local)` returns no symbol (MCT probe)
- [ ] DA06 Parameter: `find_symbol(input)` returns no symbol (MCT probe)
**Relations**

- [x] RE01 Calls: `p_top` → `p_helper`
- [x] RE04 Imports: dot-sourcing → edge
- [ ] RE05 Class extends class: PowerShell classes are not indexed (corpus test)
**Search and lookup**

- [x] SE01 Find a definition: `list_symbols`
- [x] SE02 Callers and callees: `find_calls p_top`
- [x] SE03 Every use: references to `p_helper`
- [x] SE04 Blast radius and context: `build_context_pack(P_Helper)` returned its caller
- [ ] SE05 Exact text in strings: literal not collected
**Code quality**

- [x] QU01 Dead code: unused function reported
- [ ] QU02 Repeated code: listed under `unsupported_languages`
**Structure**

- [~] ST01 Import graph: dot-sourced path also listed as external
- [~] ST02 File outline: declaration line and placeholder
- [ ] ST03 Declared dependencies: `.psd1` module manifest is not reported by status

**Not applicable:**

- — FN06 Destructor / finalizer: PowerShell has no destructor / finalizer
- — FN07 Operator overload: PowerShell has no operator overload
- — FN08 Abstract / pure virtual method: PowerShell has no abstract / pure virtual method
- — FN10 Overloads kept apart: PowerShell has no overloads kept apart
- — TY02 Abstract class: PowerShell has no abstract class
- — TY03 Struct: PowerShell has no struct
- — TY04 Union: PowerShell has no union
- — TY05 Interface / protocol: PowerShell has no interface / protocol
- — TY06 Interface members: PowerShell has no interface members
- — TY07 Trait / mixin: PowerShell has no trait / mixin
- — TY10 Record / data class: PowerShell has no record / data class
- — TY11 Nested / inner class: PowerShell has no nested / inner class
- — TY12 Anonymous class: PowerShell has no anonymous class
- — TY13 Partial class: PowerShell has no partial class
- — TY14 Singleton / companion object: PowerShell has no singleton / companion object
- — TY15 Type alias: PowerShell has no type alias
- — DA02 Constructor-promoted field: PowerShell has no constructor-promoted field
- — DA03 Constant (module or global): PowerShell has no constant (module or global)
- — DA07 Macro / preprocessor define: PowerShell has no macro / preprocessor define
- — RE02 Constructor call: PowerShell has no constructor call
- — RE03 Super / base constructor call: PowerShell has no super / base constructor call
- — RE06 Implements interface / trait: PowerShell has no implements interface / trait
- — RE07 Interface extends interface / supertrait: PowerShell has no interface extends interface / supertrait
- — RE08 Decorator / annotation / attribute: PowerShell has no decorator / annotation / attribute
- — RE09 Struct / type embedding: PowerShell has no struct / type embedding

Markup features (MK) do not apply to code languages.

## Markup and style languages

### HTML (`html`)

Done 3 · Partial 1 · Not done 3 · Not verified 0 · Not applicable 0

- [x] MK01 Find a definition: by `id`
- [x] MK02 Find references: `#id` and `.class` of an identified element
- [ ] MK03 Dead code: markup-only fixture returns no candidates
- [ ] MK04 Repeated code: markup-only duplicate report returns no candidates
- [~] MK05 Import graph: `<link>` edge to CSS; script source not resolved
- [x] MK06 File outline: `get_file_skeleton` returned the HTML outline
- [ ] MK07 Exact text in strings: exact-phrase search found no HTML text

Constructs:

- [x] element with an `id`
- [ ] element with only a `class`: parser corpus confirms elements without `id` are not indexed
- [ ] inline `<style>` content
- [ ] event attributes such as `onclick`
- [ ] template syntax such as `{% %}`

**Not applicable:**

- none

**Code features (not applicable to markup):**

- — Functions and methods: HTML has no code constructs (FN01 Top-level function; FN02 Nested function; FN03 Lambda or closure assigned to a name; FN04 Method; FN05 Constructor; FN06 Destructor / finalizer; FN07 Operator overload; FN08 Abstract / pure virtual method; FN09 Property accessors (get/set); FN10 Overloads kept apart)
- — Types and object-oriented programming: HTML has no code constructs (TY01 Class; TY02 Abstract class; TY03 Struct; TY04 Union; TY05 Interface / protocol; TY06 Interface members; TY07 Trait / mixin; TY08 Enum; TY09 Enum members; TY10 Record / data class; TY11 Nested / inner class; TY12 Anonymous class; TY13 Partial class; TY14 Singleton / companion object; TY15 Type alias; TY16 Module / namespace / package)
- — Fields, constants and variables: HTML has no code constructs (DA01 Field / instance attribute; DA02 Constructor-promoted field; DA03 Constant (module or global); DA04 Global / module-level variable; DA05 Local variable; DA06 Parameter; DA07 Macro / preprocessor define)
- — Relations: HTML has no code constructs (RE01 Calls; RE02 Constructor call; RE03 Super / base constructor call; RE04 Imports; RE05 Class extends class; RE06 Implements interface / trait; RE07 Interface extends interface / supertrait; RE08 Decorator / annotation / attribute; RE09 Struct / type embedding)

Search, quality and structure features that apply to markup are the MK features above.

### CSS (`css`)

Done 3 · Partial 1 · Not done 3 · Not verified 0 · Not applicable 0

- [x] MK01 Find a definition: by selector
- [x] MK02 Find references: HTML class and ID selectors resolve to their CSS elements (HTML integration tests)
- [ ] MK03 Dead code: markup-only fixture returns no candidates
- [ ] MK04 Repeated code: markup-only duplicate report returns no candidates
- [~] MK05 Import graph: local `@import` edge, also listed as external
- [x] MK06 File outline: `get_file_skeleton` returned the CSS outline
- [ ] MK07 Exact text in strings: exact-phrase search found no CSS source text

Constructs:

- [x] rule for each full selector and atom
- [x] nested rules: corpus resolves nested selectors and preserves their ranges
- [x] rules inside `@media`
- [ ] declarations and values: CSS indexing emits selectors, not declaration/value symbols
- [ ] custom properties (`--p_brand`)
- [ ] `@keyframes`
- [ ] pseudo-classes as atoms

**Not applicable:**

- none

**Code features (not applicable to markup):**

- — Functions and methods: CSS has no code constructs (FN01 Top-level function; FN02 Nested function; FN03 Lambda or closure assigned to a name; FN04 Method; FN05 Constructor; FN06 Destructor / finalizer; FN07 Operator overload; FN08 Abstract / pure virtual method; FN09 Property accessors (get/set); FN10 Overloads kept apart)
- — Types and object-oriented programming: CSS has no code constructs (TY01 Class; TY02 Abstract class; TY03 Struct; TY04 Union; TY05 Interface / protocol; TY06 Interface members; TY07 Trait / mixin; TY08 Enum; TY09 Enum members; TY10 Record / data class; TY11 Nested / inner class; TY12 Anonymous class; TY13 Partial class; TY14 Singleton / companion object; TY15 Type alias; TY16 Module / namespace / package)
- — Fields, constants and variables: CSS has no code constructs (DA01 Field / instance attribute; DA02 Constructor-promoted field; DA03 Constant (module or global); DA04 Global / module-level variable; DA05 Local variable; DA06 Parameter; DA07 Macro / preprocessor define)
- — Relations: CSS has no code constructs (RE01 Calls; RE02 Constructor call; RE03 Super / base constructor call; RE04 Imports; RE05 Class extends class; RE06 Implements interface / trait; RE07 Interface extends interface / supertrait; RE08 Decorator / annotation / attribute; RE09 Struct / type embedding)

Search, quality and structure features that apply to markup are the MK features above.

### XML (`xml`)

Done 2 · Partial 0 · Not done 5 · Not verified 0 · Not applicable 0

- [x] MK01 Find a definition: by `id`, `name` or `Name`
- [ ] MK02 Find references: XML emits no relations (confirmed by corpus and parse tests)
- [ ] MK03 Dead code: markup-only fixture returns no candidates
- [ ] MK04 Repeated code: markup-only duplicate report returns no candidates
- [ ] MK05 Import graph: XML emits no imports or other relations (confirmed by tests)
- [x] MK06 File outline: `get_file_skeleton` returned the XML outline
- [ ] MK07 Exact text in strings: exact-phrase search found no XML text

Constructs:

- [x] element with `id` or `name`
- [ ] unnamed elements
- [ ] attributes
- [ ] text

**Not applicable:**

- none

**Code features (not applicable to markup):**

- — Functions and methods: XML has no code constructs (FN01 Top-level function; FN02 Nested function; FN03 Lambda or closure assigned to a name; FN04 Method; FN05 Constructor; FN06 Destructor / finalizer; FN07 Operator overload; FN08 Abstract / pure virtual method; FN09 Property accessors (get/set); FN10 Overloads kept apart)
- — Types and object-oriented programming: XML has no code constructs (TY01 Class; TY02 Abstract class; TY03 Struct; TY04 Union; TY05 Interface / protocol; TY06 Interface members; TY07 Trait / mixin; TY08 Enum; TY09 Enum members; TY10 Record / data class; TY11 Nested / inner class; TY12 Anonymous class; TY13 Partial class; TY14 Singleton / companion object; TY15 Type alias; TY16 Module / namespace / package)
- — Fields, constants and variables: XML has no code constructs (DA01 Field / instance attribute; DA02 Constructor-promoted field; DA03 Constant (module or global); DA04 Global / module-level variable; DA05 Local variable; DA06 Parameter; DA07 Macro / preprocessor define)
- — Relations: XML has no code constructs (RE01 Calls; RE02 Constructor call; RE03 Super / base constructor call; RE04 Imports; RE05 Class extends class; RE06 Implements interface / trait; RE07 Interface extends interface / supertrait; RE08 Decorator / annotation / attribute; RE09 Struct / type embedding)

Search, quality and structure features that apply to markup are the MK features above.

### XAML (`xaml`)

Done 3 · Partial 1 · Not done 3 · Not verified 0 · Not applicable 0

- [x] MK01 Find a definition: by `x:Name`
- [x] MK02 Find references: event handler names
- [ ] MK03 Dead code: markup-only fixture returns no candidates
- [ ] MK04 Repeated code: markup-only duplicate report returns no candidates
- [~] MK05 Import graph: relative source resolves; absolute `pack:` source emits no import relation
- [x] MK06 File outline: `get_file_skeleton` returned the XAML outline
- [ ] MK07 Exact text in strings: exact-phrase search found no XAML text

Constructs:

- [x] `x:Name`
- [x] event handler `Click` → code-behind name (unresolved here)
- [x] relative `ResourceDictionary Source`
- [ ] `{Binding …}` values
- [ ] absolute `pack:` resource URI produces no import relation (MCT probe)
- [ ] other attributes

**Not applicable:**

- none

**Code features (not applicable to markup):**

- — Functions and methods: XAML has no code constructs (FN01 Top-level function; FN02 Nested function; FN03 Lambda or closure assigned to a name; FN04 Method; FN05 Constructor; FN06 Destructor / finalizer; FN07 Operator overload; FN08 Abstract / pure virtual method; FN09 Property accessors (get/set); FN10 Overloads kept apart)
- — Types and object-oriented programming: XAML has no code constructs (TY01 Class; TY02 Abstract class; TY03 Struct; TY04 Union; TY05 Interface / protocol; TY06 Interface members; TY07 Trait / mixin; TY08 Enum; TY09 Enum members; TY10 Record / data class; TY11 Nested / inner class; TY12 Anonymous class; TY13 Partial class; TY14 Singleton / companion object; TY15 Type alias; TY16 Module / namespace / package)
- — Fields, constants and variables: XAML has no code constructs (DA01 Field / instance attribute; DA02 Constructor-promoted field; DA03 Constant (module or global); DA04 Global / module-level variable; DA05 Local variable; DA06 Parameter; DA07 Macro / preprocessor define)
- — Relations: XAML has no code constructs (RE01 Calls; RE02 Constructor call; RE03 Super / base constructor call; RE04 Imports; RE05 Class extends class; RE06 Implements interface / trait; RE07 Interface extends interface / supertrait; RE08 Decorator / annotation / attribute; RE09 Struct / type embedding)

Search, quality and structure features that apply to markup are the MK features above.

### Markdown (`markdown`)

Done 3 · Partial 1 · Not done 3 · Not verified 0 · Not applicable 0

- [x] MK01 Find a definition: notes and headings
- [x] MK02 Find references: wiki links and anchors
- [ ] MK03 Dead code: markup-only fixture returns no candidates
- [ ] MK04 Repeated code: markup-only duplicate report returns no candidates
- [~] MK05 Import graph: only `![[embeds]]` are edges
- [x] MK06 File outline: `get_file_skeleton` returned the Markdown heading outline
- [ ] MK07 Exact text in strings: exact-phrase search found no Markdown paragraph text

Constructs:

- [x] note module and headings with `level`
- [x] `[[WikiLink]]` references
- [x] `#anchor` links resolve note and heading relations (MCT integration tests)
- [ ] inline `#tags` not found by name
- [x] frontmatter tags, aliases and title are queryable (Markdown corpus tests)
- [x] `![[embed]]` imports
- [x] code spans and tables: links/tags in code are ignored and table links/tags are scanned (corpus tests)

**Not applicable:**

- none

**Code features (not applicable to markup):**

- — Functions and methods: Markdown has no code constructs (FN01 Top-level function; FN02 Nested function; FN03 Lambda or closure assigned to a name; FN04 Method; FN05 Constructor; FN06 Destructor / finalizer; FN07 Operator overload; FN08 Abstract / pure virtual method; FN09 Property accessors (get/set); FN10 Overloads kept apart)
- — Types and object-oriented programming: Markdown has no code constructs (TY01 Class; TY02 Abstract class; TY03 Struct; TY04 Union; TY05 Interface / protocol; TY06 Interface members; TY07 Trait / mixin; TY08 Enum; TY09 Enum members; TY10 Record / data class; TY11 Nested / inner class; TY12 Anonymous class; TY13 Partial class; TY14 Singleton / companion object; TY15 Type alias; TY16 Module / namespace / package)
- — Fields, constants and variables: Markdown has no code constructs (DA01 Field / instance attribute; DA02 Constructor-promoted field; DA03 Constant (module or global); DA04 Global / module-level variable; DA05 Local variable; DA06 Parameter; DA07 Macro / preprocessor define)
- — Relations: Markdown has no code constructs (RE01 Calls; RE02 Constructor call; RE03 Super / base constructor call; RE04 Imports; RE05 Class extends class; RE06 Implements interface / trait; RE07 Interface extends interface / supertrait; RE08 Decorator / annotation / attribute; RE09 Struct / type embedding)

Search, quality and structure features that apply to markup are the MK features above.
