extends RefCounted
## An implicit script class: no class_name, so it is reached through preload.

signal changed

const EMPTY_WEIGHT := 0

var capacity: int
var slots: Array[Slot] = []


class Slot:
    var item: Resource
    var count := 1

    func _init(start_item: Resource) -> void:
        item = start_item

    func weight() -> int:
        return count * 2


func _init(start_capacity: int = 10) -> void:
    capacity = start_capacity


func add(item: Resource) -> bool:
    if slots.size() >= capacity:
        return false
    slots.append(Slot.new(item))
    changed.emit()
    return true


func remove(item: Resource) -> void:
    slots = slots.filter(func(slot): return slot.item != item)
    changed.emit()


func total_weight() -> int:
    var weight := EMPTY_WEIGHT
    for slot in slots:
        weight += slot.weight()
    return weight
