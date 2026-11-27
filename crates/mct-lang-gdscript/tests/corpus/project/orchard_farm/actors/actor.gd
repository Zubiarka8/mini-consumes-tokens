## Base class of every character that walks the orchard: the farmer, the
## villagers and the crows. Subclasses extend this file by path.
@abstract
class_name OrchardActor
extends CharacterBody2D

signal health_changed(current: int, maximum: int)
signal died
signal stamina_depleted

enum Facing { DOWN, LEFT, RIGHT, UP }
enum MoveState { IDLE, WALKING, RUNNING, WORKING, KNOCKED_OUT }

const RUN_MULTIPLIER := 1.6
const STAMINA_REGEN_PER_SECOND := 4.0
const KNOCKOUT_SECONDS := 3.0

@export var display_name := "Villager"
@export_range(1, 500) var max_health := 100
@export_range(10.0, 400.0) var walk_speed := 90.0
@export var max_stamina := 100.0

@onready var sprite: AnimatedSprite2D = $Sprite
@onready var shadow: Sprite2D = $Shadow

var facing := Facing.DOWN
var state := MoveState.IDLE
var _knockout_left := 0.0
var _invulnerable := false

var health: int = 100:
    get:
        return health
    set(value):
        var clamped := clampi(value, 0, max_health)
        if clamped == health:
            return
        health = clamped
        health_changed.emit(health, max_health)
        if health == 0:
            _knock_out()

var stamina: float = 100.0:
    set = set_stamina


func _init() -> void:
    health = max_health
    stamina = max_stamina


func _ready() -> void:
    _apply_facing()


func _physics_process(delta: float) -> void:
    if state == MoveState.KNOCKED_OUT:
        _knockout_left -= delta
        if _knockout_left <= 0.0:
            _recover()
        return
    var direction := wanted_direction()
    var running := wants_to_run() and stamina > 0.0
    _update_state(direction, running)
    var speed := walk_speed * (RUN_MULTIPLIER if running else 1.0)
    velocity = direction * speed
    move_and_slide()
    _tick_stamina(delta, running)


## Where this actor wants to go this frame. The farmer reads input, the
## villagers follow their schedule and the crows chase crops.
@abstract func wanted_direction() -> Vector2


## Whether the actor wants to run. Most actors never do.
func wants_to_run() -> bool:
    return false


func set_stamina(value: float) -> void:
    var previous := stamina
    stamina = clampf(value, 0.0, max_stamina)
    if previous > 0.0 and stamina == 0.0:
        stamina_depleted.emit()


func _tick_stamina(delta: float, running: bool) -> void:
    if running:
        stamina -= delta * 12.0
    elif state != MoveState.WORKING:
        stamina += delta * STAMINA_REGEN_PER_SECOND


func _update_state(direction: Vector2, running: bool) -> void:
    if state == MoveState.WORKING:
        return
    if direction == Vector2.ZERO:
        state = MoveState.IDLE
    elif running:
        state = MoveState.RUNNING
    else:
        state = MoveState.WALKING
    if direction != Vector2.ZERO:
        facing = facing_for(direction)
    _apply_facing()


static func facing_for(direction: Vector2) -> Facing:
    if absf(direction.x) > absf(direction.y):
        return Facing.RIGHT if direction.x > 0.0 else Facing.LEFT
    return Facing.DOWN if direction.y > 0.0 else Facing.UP


func _apply_facing() -> void:
    if sprite == null:
        return
    var animation := animation_name()
    if sprite.animation != animation:
        sprite.play(animation)
    sprite.flip_h = facing == Facing.LEFT


func animation_name() -> String:
    var prefix := ""
    match state:
        MoveState.IDLE:
            prefix = "idle"
        MoveState.WALKING:
            prefix = "walk"
        MoveState.RUNNING:
            prefix = "run"
        MoveState.WORKING:
            prefix = "work"
        MoveState.KNOCKED_OUT:
            return "knocked_out"
    match facing:
        Facing.UP:
            return prefix + "_up"
        Facing.DOWN:
            return prefix + "_down"
        _:
            return prefix + "_side"


## Hurts the actor unless it is invulnerable. Returns the damage dealt.
func take_damage(amount: int, source: Node = null) -> int:
    if _invulnerable or amount <= 0 or state == MoveState.KNOCKED_OUT:
        return 0
    var dealt := mini(amount, health)
    health -= dealt
    _flash_hurt()
    if source != null:
        _on_hurt_by(source)
    return dealt


func heal(amount: int) -> void:
    if amount > 0 and state != MoveState.KNOCKED_OUT:
        health += amount


func is_knocked_out() -> bool:
    return state == MoveState.KNOCKED_OUT


func _knock_out() -> void:
    state = MoveState.KNOCKED_OUT
    _knockout_left = KNOCKOUT_SECONDS
    velocity = Vector2.ZERO
    _apply_facing()
    died.emit()


func _recover() -> void:
    state = MoveState.IDLE
    health = max_health / 2
    _invulnerable = true
    await get_tree().create_timer(1.0).timeout
    _invulnerable = false


func _flash_hurt() -> void:
    if sprite == null:
        return
    var tween := create_tween()
    tween.tween_property(sprite, "modulate", Color(1, 0.4, 0.4), 0.05)
    tween.tween_property(sprite, "modulate", Color.WHITE, 0.2)


## Hook for subclasses: villagers complain, crows fly away.
func _on_hurt_by(_source: Node) -> void:
    pass


## Starts a timed job (pruning, picking, watering). The actor stands still
## until it finishes.
func begin_work(seconds: float, label: String) -> bool:
    if state == MoveState.KNOCKED_OUT or state == MoveState.WORKING:
        return false
    if stamina < seconds * 2.0:
        return false
    state = MoveState.WORKING
    velocity = Vector2.ZERO
    _apply_facing()
    stamina -= seconds * 2.0
    await get_tree().create_timer(seconds).timeout
    if state == MoveState.WORKING:
        state = MoveState.IDLE
        _apply_facing()
        finish_work(label)
    return true


## Hook for subclasses when a job started with `begin_work` completes.
func finish_work(_label: String) -> void:
    pass


func distance_to_actor(other: OrchardActor) -> float:
    return global_position.distance_to(other.global_position)


func tile_position(tile_size: int = 16) -> Vector2i:
    return Vector2i(floori(global_position.x / tile_size), floori(global_position.y / tile_size))


func tile_in_front(tile_size: int = 16) -> Vector2i:
    var tile := tile_position(tile_size)
    match facing:
        Facing.UP:
            return tile + Vector2i.UP
        Facing.DOWN:
            return tile + Vector2i.DOWN
        Facing.LEFT:
            return tile + Vector2i.LEFT
        _:
            return tile + Vector2i.RIGHT


func status_line() -> String:
    var label := "%s: %d/%d hp, %d stamina" % [display_name, health, max_health, int(stamina)]
    if is_knocked_out():
        label += " (knocked out)"
    return label


func to_save_data() -> Dictionary:
    return {
        "name": display_name,
        "health": health,
        "stamina": stamina,
        "x": global_position.x,
        "y": global_position.y,
        "facing": facing,
    }


func load_save_data(data: Dictionary) -> void:
    display_name = data.get("name", display_name)
    health = int(data.get("health", max_health))
    stamina = float(data.get("stamina", max_stamina))
    global_position = Vector2(data.get("x", 0.0), data.get("y", 0.0))
    facing = int(data.get("facing", Facing.DOWN))
    _apply_facing()


func debug_dump() -> Dictionary:
    var dump := to_save_data()
    dump["state"] = MoveState.keys()[state]
    dump["invulnerable"] = _invulnerable
    dump["knockout_left"] = _knockout_left
    return dump


func is_near(point: Vector2, radius: float = 24.0) -> bool:
    return global_position.distance_squared_to(point) <= radius * radius


func look_at_point(point: Vector2) -> void:
    var direction := (point - global_position).normalized()
    if direction != Vector2.ZERO:
        facing = facing_for(direction)
        _apply_facing()


func stop() -> void:
    velocity = Vector2.ZERO
    if state != MoveState.KNOCKED_OUT:
        state = MoveState.IDLE
    _apply_facing()


## Pushes the actor away from `origin`, for a falling branch or a charging
## goat. The push fades over `seconds`.
func knock_back(origin: Vector2, strength: float, seconds: float = 0.25) -> void:
    if state == MoveState.KNOCKED_OUT or strength <= 0.0:
        return
    var away := (global_position - origin).normalized()
    if away == Vector2.ZERO:
        away = Vector2.DOWN
    var tween := create_tween()
    tween.tween_property(self, "global_position", global_position + away * strength, seconds)
    look_at_point(origin)
