@abstract
class_name Entity
extends Node2D
## Base of everything that lives on the dungeon grid.

signal died(entity: Entity)
signal health_changed(old_value: int, new_value: int)

enum Team { PLAYER, MONSTER, NEUTRAL = 9 }
enum { FACING_LEFT, FACING_RIGHT }

const MAX_HEALTH := 100
const Bag = preload("res://items/inventory.gd")

static var spawned := 0

@export var team: Team = Team.NEUTRAL
@export_range(1, 10) var speed := 4
@onready var sprite = $Sprite2D

var facing := FACING_RIGHT
var bag := Bag.new(8)
var health: int = MAX_HEALTH:
    get:
        return health
    set(value):
        var old := health
        health = clampi(value, 0, MAX_HEALTH)
        health_changed.emit(old, health)
        if health == 0:
            died.emit(self)
var armor: int:
    get = get_armor, set = set_armor


func _init(start_team: Team = Team.NEUTRAL, ...tags: Array) -> void:
    team = start_team
    spawned += 1
    for tag: String in tags:
        add_to_group(tag)


func _ready() -> void:
    GameState.register(self)


@abstract func take_turn() -> void


static func count() -> int:
    return spawned


func take_damage(amount: int) -> int:
    var dealt := maxi(amount - armor, 0)
    health -= dealt
    return dealt


func is_alive() -> bool:
    return health > 0


func get_armor() -> int:
    return bag.total_weight() / 10


func set_armor(_value: int) -> void:
    pass
