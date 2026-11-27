## Orchard Farm item catalog.
##
## Read by `mct-lang-gdscript` (tree-sitter-gdscript 6.1). The scenario is a
## small farming game written for Godot 4.4: every script under
## `orchard_farm/` is owned application code, and engine classes (Node,
## RefCounted, HTTPRequest, JSON...) stay external.
##
## Relations the parser is expected to extract here: the `ItemDef` inner
## class and its constructor calls, the static lookups other scripts call
## through `ItemCatalog`, the enums used by the inventory and the market
## client, and the string literals of item names for exact text search.
class_name ItemCatalog
extends RefCounted

## Broad families the shop and the inventory sort by.
enum Category { SEED, CROP, TOOL, FERTILIZER, PRODUCE, DECOR }

## Quality tiers, from a bruised crop to a prize winner.
enum Quality { POOR, NORMAL, GOOD, PRIZE }

## Seasons a seed can be planted in. Bit flags, so one seed may allow several.
enum { SPRING = 1, SUMMER = 2, AUTUMN = 4, WINTER = 8 }

const MAX_STACK := 99
const TOOL_STACK := 1
const PRICE_ROUNDING := 5
const QUALITY_MULTIPLIERS := {
    Quality.POOR: 0.5,
    Quality.NORMAL: 1.0,
    Quality.GOOD: 1.25,
    Quality.PRIZE: 2.0,
}


## One entry of the catalog. Entries are immutable once registered.
class ItemDef:
    var id: StringName
    var display_name: String
    var category: Category
    var base_price: int
    var stack_limit: int
    var seasons: int
    var grows_into: StringName
    var days_to_grow: int

    func _init(
        item_id: StringName,
        item_name: String,
        item_category: Category,
        price: int,
        limit: int = MAX_STACK
    ) -> void:
        id = item_id
        display_name = item_name
        category = item_category
        base_price = price
        stack_limit = limit
        seasons = 0
        grows_into = &""
        days_to_grow = 0

    func is_seed() -> bool:
        return category == Category.SEED

    func is_tool() -> bool:
        return category == Category.TOOL

    func can_plant_in(season: int) -> bool:
        return is_seed() and (seasons & season) != 0

    func describe() -> String:
        var parts: Array[String] = [display_name]
        if is_seed():
            parts.append("grows in %d days" % days_to_grow)
        if stack_limit == TOOL_STACK:
            parts.append("does not stack")
        return ", ".join(parts)

    func to_dict() -> Dictionary:
        return {
            "id": String(id),
            "name": display_name,
            "category": category,
            "price": base_price,
            "stack": stack_limit,
        }


static var _items: Dictionary = {}
static var _loaded := false


## Fills the catalog with the starter items. Safe to call more than once.
static func ensure_loaded() -> void:
    if _loaded:
        return
    _register_seeds()
    _register_crops()
    _register_tools()
    _register_supplies()
    _loaded = true


static func _register_seeds() -> void:
    _add_seed(&"apple_seed", "Apple sapling", 120, &"apple", 12, SPRING)
    _add_seed(&"pear_seed", "Pear sapling", 140, &"pear", 14, SPRING | SUMMER)
    _add_seed(&"plum_seed", "Plum sapling", 110, &"plum", 10, SUMMER)
    _add_seed(&"cherry_seed", "Cherry sapling", 160, &"cherry", 16, SPRING)
    _add_seed(&"quince_seed", "Quince sapling", 90, &"quince", 9, AUTUMN)
    _add_seed(&"pumpkin_seed", "Pumpkin seeds", 30, &"pumpkin", 7, SUMMER | AUTUMN)
    _add_seed(&"kale_seed", "Kale seeds", 15, &"kale", 4, AUTUMN | WINTER)


static func _register_crops() -> void:
    _add(ItemDef.new(&"apple", "Apple", Category.CROP, 25))
    _add(ItemDef.new(&"pear", "Pear", Category.CROP, 28))
    _add(ItemDef.new(&"plum", "Plum", Category.CROP, 22))
    _add(ItemDef.new(&"cherry", "Cherry", Category.CROP, 35))
    _add(ItemDef.new(&"quince", "Quince", Category.CROP, 30))
    _add(ItemDef.new(&"pumpkin", "Pumpkin", Category.CROP, 40))
    _add(ItemDef.new(&"kale", "Kale", Category.CROP, 12))
    _add(ItemDef.new(&"cider", "Cider bottle", Category.PRODUCE, 90))
    _add(ItemDef.new(&"jam", "Plum jam", Category.PRODUCE, 75))


static func _register_tools() -> void:
    for entry in [
        [&"pruning_shears", "Pruning shears", 250],
        [&"watering_can", "Watering can", 180],
        [&"ladder", "Orchard ladder", 400],
        [&"basket", "Harvest basket", 60],
    ]:
        _add(ItemDef.new(entry[0], entry[1], Category.TOOL, entry[2], TOOL_STACK))


static func _register_supplies() -> void:
    _add(ItemDef.new(&"compost", "Compost sack", Category.FERTILIZER, 20))
    _add(ItemDef.new(&"bone_meal", "Bone meal", Category.FERTILIZER, 35))
    _add(ItemDef.new(&"scarecrow", "Scarecrow", Category.DECOR, 150, TOOL_STACK))
    _add(ItemDef.new(&"lantern", "Orchard lantern", Category.DECOR, 95, 10))


static func _add_seed(
    id: StringName,
    name: String,
    price: int,
    crop: StringName,
    days: int,
    season_flags: int
) -> void:
    var item := ItemDef.new(id, name, Category.SEED, price)
    item.grows_into = crop
    item.days_to_grow = days
    item.seasons = season_flags
    _add(item)


static func _add(item: ItemDef) -> void:
    assert(not _items.has(item.id), "duplicate catalog id")
    _items[item.id] = item


## The definition for `id`, or null when the catalog has no such item.
static func find(id: StringName) -> ItemDef:
    ensure_loaded()
    return _items.get(id)


static func has(id: StringName) -> bool:
    ensure_loaded()
    return _items.has(id)


static func stack_limit_of(id: StringName) -> int:
    var item := find(id)
    if item == null:
        return 0
    return item.stack_limit


## Every item of one category, sorted by display name.
static func in_category(category: Category) -> Array[ItemDef]:
    ensure_loaded()
    var result: Array[ItemDef] = []
    for item: ItemDef in _items.values():
        if item.category == category:
            result.append(item)
    result.sort_custom(func(a: ItemDef, b: ItemDef): return a.display_name < b.display_name)
    return result


## Seeds that can go in the ground this season.
static func plantable_in(season: int) -> Array[ItemDef]:
    var result: Array[ItemDef] = []
    for item in in_category(Category.SEED):
        if item.can_plant_in(season):
            result.append(item)
    return result


## The price the general store pays for one unit of `id` at `quality`.
static func sell_price(id: StringName, quality: Quality = Quality.NORMAL) -> int:
    var item := find(id)
    if item == null or item.is_tool():
        return 0
    var multiplier: float = QUALITY_MULTIPLIERS.get(quality, 1.0)
    var raw := int(item.base_price * multiplier)
    return round_price(raw)


## The price the store asks when the player buys one unit.
static func buy_price(id: StringName) -> int:
    var item := find(id)
    if item == null:
        return 0
    var markup := 1.5 if item.category == Category.SEED else 1.2
    return round_price(int(item.base_price * markup))


static func round_price(value: int) -> int:
    if value <= 0:
        return 0
    var remainder := value % PRICE_ROUNDING
    if remainder == 0:
        return value
    return value + PRICE_ROUNDING - remainder


static func quality_name(quality: Quality) -> String:
    match quality:
        Quality.POOR:
            return "Bruised"
        Quality.NORMAL:
            return "Fresh"
        Quality.GOOD:
            return "Choice"
        Quality.PRIZE:
            return "Prize winner"
    return "Unknown"


static func season_name(flag: int) -> String:
    match flag:
        SPRING:
            return "Spring"
        SUMMER:
            return "Summer"
        AUTUMN:
            return "Autumn"
        WINTER:
            return "Winter"
        _:
            return "Any season"


## Season flags spelled out, for the seed tooltip.
static func seasons_label(flags: int) -> String:
    var names: Array[String] = []
    for flag in [SPRING, SUMMER, AUTUMN, WINTER]:
        if flags & flag:
            names.append(season_name(flag))
    if names.is_empty():
        return "Never"
    return " / ".join(names)


## The crop a seed turns into, or an empty name for anything else.
static func harvest_of(seed_id: StringName) -> StringName:
    var item := find(seed_id)
    if item == null or not item.is_seed():
        return &""
    return item.grows_into


## Items a crafting station accepts as input for `product`.
static func recipe_inputs(product: StringName) -> Dictionary:
    match product:
        &"cider":
            return {&"apple": 6}
        &"jam":
            return {&"plum": 4}
        _:
            return {}


## Catalog contents as plain data for the save file and the market sync.
static func export_all() -> Array[Dictionary]:
    ensure_loaded()
    var rows: Array[Dictionary] = []
    for id in _items:
        var item: ItemDef = _items[id]
        rows.append(item.to_dict())
    rows.sort_custom(func(a, b): return a["id"] < b["id"])
    return rows


## Applies prices the market server sent. Unknown ids are ignored.
static func apply_price_update(prices: Dictionary) -> int:
    ensure_loaded()
    var changed := 0
    for id in prices:
        var item: ItemDef = _items.get(StringName(id))
        if item == null:
            continue
        var price := int(prices[id])
        if price > 0 and price != item.base_price:
            item.base_price = price
            changed += 1
    return changed


## Clears the catalog. Only tests and the editor reload tool call this.
static func reset() -> void:
    _items.clear()
    _loaded = false
