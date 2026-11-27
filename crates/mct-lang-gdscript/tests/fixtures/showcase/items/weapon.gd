class_name Weapon
extends Resource
## A weapon with a damage value guarded by its setter.

enum Kind { BLADE, BOW, STAFF }

const MIN_DAMAGE := 1

@export var title := "stick"
@export var kind: Kind = Kind.BLADE
var damage: int = MIN_DAMAGE:
    set(value):
        damage = maxi(value, MIN_DAMAGE)
    get:
        return damage
var uses := 0


func _init(start_title: String = "stick", start_damage: int = MIN_DAMAGE) -> void:
    title = start_title
    damage = start_damage


static func create(new_title: String, new_damage: int) -> Weapon:
    return Weapon.new(new_title, new_damage)


func strike(power: int) -> int:
    uses += 1
    return damage + power


func describe() -> String:
    return "%s (%d)" % [title, damage]
