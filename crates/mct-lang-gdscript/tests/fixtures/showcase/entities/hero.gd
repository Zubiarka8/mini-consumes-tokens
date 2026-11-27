class_name Hero
extends Entity

const Pack = preload("res://items/inventory.gd")

var weapon: Weapon
var stats := Stats.new()
var on_level_up = func(level: int): return level * 2


class Stats:
    var strength := 3
    var luck := 1

    func roll() -> int:
        return strength + luck

    class Bonus:
        func apply(stats: Stats) -> void:
            stats.luck += 1


func _init() -> void:
    super(Team.PLAYER, "heroes")
    weapon = Weapon.create("sword", 7)


func _ready() -> void:
    super._ready()
    var pack := Pack.new(4)
    pack.add(weapon)


func take_turn() -> void:
    var outcome = weapon.strike(stats.roll())
    match outcome:
        0:
            print("Hero missed")
        [var first, ..]:
            print(first)
        {"crit": var crit}:
            print(crit)
        _:
            GameState.record_hit(outcome)
    var heal = func(points): return points + 1
    take_damage(-heal.call(on_level_up.call(2)))


func take_damage(amount: int) -> int:
    var dealt := super.take_damage(amount)
    if not is_alive():
        GameState.end_run(self)
    return dealt
