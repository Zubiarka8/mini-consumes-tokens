## Daily weather roll. The grid listens for rain; the HUD shows the forecast.
extends Node

signal rain_started
signal rain_stopped
signal forecast_changed(kind: Kind)

enum Kind { SUNNY, CLOUDY, RAIN, STORM, DROUGHT }

const RAIN_CHANCE := {
    1: 0.35,
    2: 0.15,
    4: 0.4,
    8: 0.2,
}

var today := Kind.SUNNY
var tomorrow := Kind.SUNNY
var _dry_streak := 0
var _rng := RandomNumberGenerator.new()


func _ready() -> void:
    _rng.seed = GameState.world_seed
    GameState.day_started.connect(_on_day_started)


func _on_day_started(_day: int) -> void:
    var was_raining := is_raining()
    today = tomorrow
    tomorrow = roll(GameState.season_flag())
    forecast_changed.emit(tomorrow)
    if is_raining() and not was_raining:
        rain_started.emit()
    elif was_raining and not is_raining():
        rain_stopped.emit()
    _dry_streak = 0 if is_raining() else _dry_streak + 1


func roll(season: int) -> Kind:
    var chance: float = RAIN_CHANCE.get(season, 0.25)
    var value := _rng.randf()
    if _dry_streak >= 6 and season == ItemCatalog.SUMMER:
        return Kind.DROUGHT
    if value < chance * 0.2:
        return Kind.STORM
    if value < chance:
        return Kind.RAIN
    if value < chance + 0.2:
        return Kind.CLOUDY
    return Kind.SUNNY


func is_raining() -> bool:
    return today == Kind.RAIN or today == Kind.STORM


func is_drought() -> bool:
    return today == Kind.DROUGHT


func label(kind: Kind = today) -> String:
    match kind:
        Kind.SUNNY:
            return "Sunny"
        Kind.CLOUDY:
            return "Cloudy"
        Kind.RAIN:
            return "Rain"
        Kind.STORM:
            return "Storm"
        Kind.DROUGHT:
            return "Drought"
    return "?"
