## Global game state, registered as the `GameState` autoload in the
## project settings. Owns the calendar, the ledger, experience and the save
## file; every other script reaches it by its autoload name.
extends Node

const SAVE_PATH := "user://orchard_save.json"
const SAVE_VERSION := 3
const DAYS_PER_SEASON := 28
const SEASONS := [ItemCatalog.SPRING, ItemCatalog.SUMMER, ItemCatalog.AUTUMN, ItemCatalog.WINTER]

signal day_started(day: int)
signal season_changed(season: int)
signal notified(text: String)
signal dialogue_requested(speaker: String)
signal level_up(skill: StringName, level: int)

var day := 1
var world_seed := 0
var harvest_totals: Dictionary = {}
var sales: Array[Dictionary] = []
var experience: Dictionary = {}
var unlocked_recipes: Array[StringName] = []
var _log: Array[String] = []
var _farmer: Farmer
var _grid: FarmGrid


func _ready() -> void:
    ItemCatalog.ensure_loaded()
    if world_seed == 0:
        world_seed = randi()


func bind_world(farmer: Farmer, grid: FarmGrid) -> void:
    _farmer = farmer
    _grid = grid
    grid.register_actor(farmer)
    farmer.harvested.connect(_on_harvested)


## Ends the day: everyone sleeps, the field grows, the season may turn.
func sleep() -> void:
    var previous := season_index()
    day += 1
    day_started.emit(day)
    var current := season_index()
    if current != previous:
        season_changed.emit(season_flag())
        notify("%s has begun." % season_label())
    save_game()


func season_index() -> int:
    return ((day - 1) / DAYS_PER_SEASON) % SEASONS.size()


func season_flag() -> int:
    return SEASONS[season_index()]


func season_label() -> String:
    return ItemCatalog.season_name(season_flag())


func day_of_season() -> int:
    return (day - 1) % DAYS_PER_SEASON + 1


func year() -> int:
    return (day - 1) / (DAYS_PER_SEASON * SEASONS.size()) + 1


func calendar_label() -> String:
    return "%s %d, year %d" % [season_label(), day_of_season(), year()]


func notify(text: String) -> void:
    _log.append(text)
    if _log.size() > 50:
        _log.pop_front()
    notified.emit(text)


func recent_messages(count: int = 5) -> Array[String]:
    return _log.slice(maxi(0, _log.size() - count))


func start_dialogue(speaker: String) -> void:
    if _farmer != null:
        _farmer.set_input_enabled(false)
    dialogue_requested.emit(speaker)


func end_dialogue() -> void:
    if _farmer != null:
        _farmer.set_input_enabled(true)


func record_harvest(crop: StringName, amount: int) -> void:
    if amount <= 0:
        return
    harvest_totals[crop] = harvest_totals.get(crop, 0) + amount
    _check_recipe_unlocks(crop)


func record_sale(id: StringName, amount: int, earned: int) -> void:
    sales.append({"day": day, "id": String(id), "amount": amount, "earned": earned})
    if sales.size() > 200:
        sales = sales.slice(sales.size() - 200)


func earnings_between(first_day: int, last_day: int) -> int:
    var total := 0
    for sale in sales:
        if sale["day"] >= first_day and sale["day"] <= last_day:
            total += int(sale["earned"])
    return total


func best_seller() -> StringName:
    var totals := {}
    for sale in sales:
        totals[sale["id"]] = totals.get(sale["id"], 0) + int(sale["earned"])
    var best := ""
    var best_total := -1
    for id in totals:
        if totals[id] > best_total:
            best = id
            best_total = totals[id]
    return StringName(best)


## Experience needed to go from `level` to the next one.
static func experience_for(level: int) -> int:
    return 10 * level * level


func skill_level(skill: StringName) -> int:
    var points: int = experience.get(skill, 0)
    var level := 1
    while points >= experience_for(level):
        points -= experience_for(level)
        level += 1
    return level


func add_experience(skill: StringName, points: int) -> void:
    var before := skill_level(skill)
    experience[skill] = experience.get(skill, 0) + points
    var after := skill_level(skill)
    if after > before:
        level_up.emit(skill, after)
        notify("Your %s skill is now level %d." % [skill, after])


func _check_recipe_unlocks(crop: StringName) -> void:
    var thresholds := {&"apple": [&"cider", 30], &"plum": [&"jam", 20]}
    if not thresholds.has(crop):
        return
    var rule: Array = thresholds[crop]
    var product: StringName = rule[0]
    if unlocked_recipes.has(product):
        return
    if harvest_totals.get(crop, 0) >= rule[1]:
        unlocked_recipes.append(product)
        notify("New recipe: %s." % ItemCatalog.find(product).display_name)


func _on_harvested(crop: StringName, amount: int) -> void:
    if amount >= 9:
        add_experience(&"orchard", 3)
    elif amount > 0:
        add_experience(&"orchard", 1)
    if harvest_totals.get(crop, 0) == amount:
        notify("First %s of the year!" % ItemCatalog.find(crop).display_name)


func save_data() -> Dictionary:
    var data := {
        "version": SAVE_VERSION,
        "day": day,
        "seed": world_seed,
        "harvest": _stringify_keys(harvest_totals),
        "sales": sales,
        "experience": _stringify_keys(experience),
        "recipes": unlocked_recipes.map(func(id): return String(id)),
    }
    if _farmer != null:
        data["farmer"] = _farmer.to_save_data()
    if _grid != null:
        data["grid"] = _grid.to_save_data()
    return data


func save_game() -> bool:
    var file := FileAccess.open(SAVE_PATH, FileAccess.WRITE)
    if file == null:
        notify("Could not save the game.")
        return false
    file.store_string(JSON.stringify(save_data(), "  "))
    return true


func load_game() -> bool:
    if not FileAccess.file_exists(SAVE_PATH):
        return false
    var text := FileAccess.get_file_as_string(SAVE_PATH)
    var parsed = JSON.parse_string(text)
    if not parsed is Dictionary:
        notify("The save file is damaged.")
        return false
    return apply_save_data(_migrate(parsed))


func _migrate(data: Dictionary) -> Dictionary:
    var version := int(data.get("version", 1))
    if version < 2:
        # Version 1 stored experience as a single number.
        data["experience"] = {"farming": data.get("experience", 0)}
    if version < 3:
        data["recipes"] = []
    data["version"] = SAVE_VERSION
    return data


func apply_save_data(data: Dictionary) -> bool:
    day = int(data.get("day", 1))
    world_seed = int(data.get("seed", world_seed))
    harvest_totals = _stringname_keys(data.get("harvest", {}))
    experience = _stringname_keys(data.get("experience", {}))
    sales.clear()
    for sale in data.get("sales", []):
        if sale is Dictionary:
            sales.append(sale)
    unlocked_recipes.clear()
    for id in data.get("recipes", []):
        unlocked_recipes.append(StringName(id))
    if _farmer != null and data.has("farmer"):
        _farmer.load_save_data(data["farmer"])
    if _grid != null and data.has("grid"):
        _grid.load_save_data(data["grid"])
    return true


static func _stringify_keys(source: Dictionary) -> Dictionary:
    var result := {}
    for key in source:
        result[String(key)] = source[key]
    return result


static func _stringname_keys(source: Dictionary) -> Dictionary:
    var result := {}
    for key in source:
        result[StringName(key)] = source[key]
    return result


func delete_save() -> void:
    if FileAccess.file_exists(SAVE_PATH):
        DirAccess.remove_absolute(ProjectSettings.globalize_path(SAVE_PATH))


func new_game(seed_value: int = 0) -> void:
    day = 1
    world_seed = seed_value if seed_value != 0 else randi()
    harvest_totals.clear()
    sales.clear()
    experience.clear()
    unlocked_recipes.clear()
    _log.clear()
    notify("Welcome to the orchard.")
    day_started.emit(day)


func summary() -> String:
    var lines: Array[String] = [calendar_label()]
    for crop in harvest_totals:
        lines.append("%s harvested: %d" % [ItemCatalog.find(crop).display_name, harvest_totals[crop]])
    for skill in experience:
        lines.append("%s level %d" % [String(skill).capitalize(), skill_level(skill)])
    if _farmer != null:
        lines.append("Net worth: %d" % _farmer.net_worth())
    return "\n".join(lines)


func is_market_day() -> bool:
    return day % 7 == 0


func price_bonus() -> float:
    return 1.2 if is_market_day() else 1.0


func days_until_season_end() -> int:
    return DAYS_PER_SEASON - day_of_season()


func can_plant_today(seed_id: StringName) -> bool:
    var item := ItemCatalog.find(seed_id)
    if item == null:
        return false
    return item.can_plant_in(season_flag()) and item.days_to_grow <= days_until_season_end()
