## Slot-based inventory shared by the farmer, the storage shed and the
## market stall. Stack limits come from `ItemCatalog`.
class_name Inventory
extends RefCounted

const Catalog = preload("./item_catalog.gd")

signal slot_changed(index: int)
signal item_added(id: StringName, amount: int)
signal item_removed(id: StringName, amount: int)
signal became_full

const DEFAULT_SIZE := 24
const HOTBAR_SIZE := 8

enum SortMode { BY_NAME, BY_CATEGORY, BY_VALUE }


## One slot: an item id, how many, and the quality of that stack.
class Slot:
    var id: StringName = &""
    var amount: int = 0
    var quality: int = ItemCatalog.Quality.NORMAL

    func is_empty() -> bool:
        return amount <= 0 or id == &""

    func clear() -> void:
        id = &""
        amount = 0
        quality = ItemCatalog.Quality.NORMAL

    func room_for(item_id: StringName, item_quality: int) -> int:
        if is_empty():
            return ItemCatalog.stack_limit_of(item_id)
        if id != item_id or quality != item_quality:
            return 0
        return ItemCatalog.stack_limit_of(item_id) - amount

    func to_dict() -> Dictionary:
        return {"id": String(id), "amount": amount, "quality": quality}

    static func from_dict(data: Dictionary) -> Slot:
        var slot := Slot.new()
        slot.id = StringName(data.get("id", ""))
        slot.amount = int(data.get("amount", 0))
        slot.quality = int(data.get("quality", ItemCatalog.Quality.NORMAL))
        return slot


var owner_name: String
var _slots: Array[Slot] = []
var locked := false:
    set(value):
        locked = value
        for index in _slots.size():
            slot_changed.emit(index)

var size: int:
    get:
        return _slots.size()


func _init(slot_count: int = DEFAULT_SIZE, name_for_owner: String = "") -> void:
    owner_name = name_for_owner
    resize(slot_count)


func resize(slot_count: int) -> void:
    while _slots.size() < slot_count:
        _slots.append(Slot.new())
    while _slots.size() > slot_count:
        var dropped: Slot = _slots.pop_back()
        if not dropped.is_empty():
            push_warning("inventory %s dropped %s" % [owner_name, dropped.id])


func slot_at(index: int) -> Slot:
    if index < 0 or index >= _slots.size():
        return null
    return _slots[index]


func hotbar() -> Array[Slot]:
    return _slots.slice(0, HOTBAR_SIZE)


## Adds up to `amount` units and returns how many did not fit.
func add(id: StringName, amount: int, quality: int = ItemCatalog.Quality.NORMAL) -> int:
    if locked or amount <= 0 or not Catalog.has(id):
        return amount
    var left := amount
    # Top up existing stacks before opening a new slot.
    for index in _slots.size():
        var slot := _slots[index]
        if slot.is_empty() or slot.id != id:
            continue
        left = _fill(index, id, left, quality)
        if left == 0:
            break
    if left > 0:
        for index in _slots.size():
            if _slots[index].is_empty():
                left = _fill(index, id, left, quality)
                if left == 0:
                    break
    var added := amount - left
    if added > 0:
        item_added.emit(id, added)
    if left > 0:
        became_full.emit()
    return left


func _fill(index: int, id: StringName, amount: int, quality: int) -> int:
    var slot := _slots[index]
    var room := slot.room_for(id, quality)
    if room <= 0:
        return amount
    var moved := mini(room, amount)
    slot.id = id
    slot.quality = quality
    slot.amount += moved
    slot_changed.emit(index)
    return amount - moved


## Removes up to `amount` units of `id`, from the last slot backwards, and
## returns how many were removed.
func remove(id: StringName, amount: int) -> int:
    if locked or amount <= 0:
        return 0
    var wanted := amount
    for index in range(_slots.size() - 1, -1, -1):
        var slot := _slots[index]
        if slot.id != id or slot.is_empty():
            continue
        var taken := mini(slot.amount, wanted)
        slot.amount -= taken
        wanted -= taken
        if slot.amount == 0:
            slot.clear()
        slot_changed.emit(index)
        if wanted == 0:
            break
    var removed := amount - wanted
    if removed > 0:
        item_removed.emit(id, removed)
    return removed


func count(id: StringName) -> int:
    var total := 0
    for slot in _slots:
        if slot.id == id:
            total += slot.amount
    return total


func has_all(requirements: Dictionary) -> bool:
    for id in requirements:
        if count(id) < int(requirements[id]):
            return false
    return true


## Takes every requirement or nothing at all.
func consume(requirements: Dictionary) -> bool:
    if not has_all(requirements):
        return false
    for id in requirements:
        remove(id, int(requirements[id]))
    return true


func free_slots() -> int:
    var free := 0
    for slot in _slots:
        if slot.is_empty():
            free += 1
    return free


func is_full() -> bool:
    return free_slots() == 0


## Moves a whole stack, swapping when the target holds something else.
func move(from_index: int, to_index: int) -> void:
    var source := slot_at(from_index)
    var target := slot_at(to_index)
    if source == null or target == null or from_index == to_index:
        return
    if not target.is_empty() and target.id == source.id and target.quality == source.quality:
        var room := ItemCatalog.stack_limit_of(source.id) - target.amount
        var moved := mini(room, source.amount)
        target.amount += moved
        source.amount -= moved
        if source.amount == 0:
            source.clear()
    else:
        _slots[from_index] = target
        _slots[to_index] = source
    slot_changed.emit(from_index)
    slot_changed.emit(to_index)


## Splits half of a stack into the first free slot.
func split(index: int) -> bool:
    var slot := slot_at(index)
    if slot == null or slot.amount < 2:
        return false
    for other in _slots.size():
        if _slots[other].is_empty():
            var half := slot.amount / 2
            _slots[other].id = slot.id
            _slots[other].quality = slot.quality
            _slots[other].amount = half
            slot.amount -= half
            slot_changed.emit(index)
            slot_changed.emit(other)
            return true
    return false


func sort(mode: SortMode = SortMode.BY_NAME) -> void:
    var filled: Array[Slot] = []
    for slot in _slots:
        if not slot.is_empty():
            filled.append(slot)
    match mode:
        SortMode.BY_NAME:
            filled.sort_custom(_by_name)
        SortMode.BY_CATEGORY:
            filled.sort_custom(_by_category)
        SortMode.BY_VALUE:
            filled.sort_custom(func(a: Slot, b: Slot): return _value_of(a) > _value_of(b))
    for index in _slots.size():
        _slots[index] = filled[index] if index < filled.size() else Slot.new()
        slot_changed.emit(index)


func _by_name(a: Slot, b: Slot) -> bool:
    return _display_name(a) < _display_name(b)


func _by_category(a: Slot, b: Slot) -> bool:
    var left := ItemCatalog.find(a.id)
    var right := ItemCatalog.find(b.id)
    if left.category == right.category:
        return _by_name(a, b)
    return left.category < right.category


func _display_name(slot: Slot) -> String:
    var item := ItemCatalog.find(slot.id)
    return item.display_name if item else String(slot.id)


func _value_of(slot: Slot) -> int:
    return ItemCatalog.sell_price(slot.id, slot.quality) * slot.amount


## What everything in this inventory would sell for.
func total_value() -> int:
    var total := 0
    for slot in _slots:
        if not slot.is_empty():
            total += _value_of(slot)
    return total


## Moves everything that fits into `other`, for the "store all" button.
func transfer_all(other: Inventory) -> int:
    var moved := 0
    for index in _slots.size():
        var slot := _slots[index]
        if slot.is_empty():
            continue
        var left := other.add(slot.id, slot.amount, slot.quality)
        moved += slot.amount - left
        if left == 0:
            slot.clear()
        else:
            slot.amount = left
        slot_changed.emit(index)
    return moved


func to_save_data() -> Array[Dictionary]:
    var rows: Array[Dictionary] = []
    for slot in _slots:
        rows.append(slot.to_dict())
    return rows


func load_save_data(rows: Array) -> void:
    resize(maxi(rows.size(), DEFAULT_SIZE))
    for index in rows.size():
        var row = rows[index]
        if row is Dictionary:
            _slots[index] = Slot.from_dict(row)
        else:
            _slots[index] = Slot.new()
        slot_changed.emit(index)


func describe() -> String:
    var lines: Array[String] = ["Inventory of %s" % owner_name]
    for slot in _slots:
        if slot.is_empty():
            continue
        var quality := ItemCatalog.quality_name(slot.quality)
        lines.append("  %s x%d (%s)" % [_display_name(slot), slot.amount, quality])
    if lines.size() == 1:
        lines.append("  (empty)")
    return "\n".join(lines)
