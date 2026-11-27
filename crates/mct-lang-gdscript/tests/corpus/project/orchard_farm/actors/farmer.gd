## The player character. Reads input, carries the inventory and works the
## orchard tiles through `FarmGrid`.
class_name Farmer
extends "./actor.gd"

const InventoryScript = preload("../items/inventory.gd")
const ToolUse = preload("./tool_use.gd")

signal tool_changed(id: StringName)
signal money_changed(amount: int)
signal harvested(crop: StringName, amount: int)

const START_MONEY := 500
const PICK_SECONDS := 0.8
const PRUNE_SECONDS := 1.5
const WATER_SECONDS := 0.6

@export var grid_path: NodePath

var inventory: Inventory
var money := START_MONEY:
    set(value):
        money = maxi(value, 0)
        money_changed.emit(money)

var selected_slot := 0
var _grid: FarmGrid
var _input_enabled := true


func _init() -> void:
    super()
    display_name = "Farmer"
    inventory = InventoryScript.new(Inventory.DEFAULT_SIZE, "Farmer")
    inventory.item_added.connect(_on_item_added)


func _ready() -> void:
    super._ready()
    _grid = get_node_or_null(grid_path) as FarmGrid
    GameState.day_started.connect(_on_day_started)
    _give_starter_kit()


func _give_starter_kit() -> void:
    if inventory.count(&"watering_can") > 0:
        return
    inventory.add(&"watering_can", 1)
    inventory.add(&"pruning_shears", 1)
    inventory.add(&"basket", 1)
    inventory.add(&"apple_seed", 3)


func wanted_direction() -> Vector2:
    if not _input_enabled:
        return Vector2.ZERO
    return Input.get_vector("move_left", "move_right", "move_up", "move_down")


func wants_to_run() -> bool:
    return _input_enabled and Input.is_action_pressed("run")


func _unhandled_input(event: InputEvent) -> void:
    if not _input_enabled:
        return
    if event.is_action_pressed("use_tool"):
        use_selected()
    elif event.is_action_pressed("next_slot"):
        select_slot(selected_slot + 1)
    elif event.is_action_pressed("previous_slot"):
        select_slot(selected_slot - 1)
    elif event is InputEventKey and event.pressed:
        var key := event as InputEventKey
        if key.keycode >= KEY_1 and key.keycode <= KEY_8:
            select_slot(key.keycode - KEY_1)


func select_slot(index: int) -> void:
    selected_slot = wrapi(index, 0, Inventory.HOTBAR_SIZE)
    var slot := inventory.slot_at(selected_slot)
    tool_changed.emit(slot.id if slot else &"")


func selected_item() -> StringName:
    var slot := inventory.slot_at(selected_slot)
    if slot == null or slot.is_empty():
        return &""
    return slot.id


## Uses whatever sits in the selected hotbar slot on the tile in front.
func use_selected() -> void:
    if _grid == null or is_knocked_out():
        return
    var tile := tile_in_front()
    var item := selected_item()
    var use := ToolUse.for_item(item)
    match use:
        ToolUse.Kind.WATER:
            await _water(tile)
        ToolUse.Kind.PRUNE:
            await _prune(tile)
        ToolUse.Kind.PICK:
            await _pick(tile)
        ToolUse.Kind.PLANT:
            _plant(tile, item)
        ToolUse.Kind.FERTILIZE:
            _fertilize(tile, item)
        _:
            _pick_or_talk(tile)


func _water(tile: Vector2i) -> void:
    if not _grid.can_water(tile):
        return
    if await begin_work(WATER_SECONDS, "water"):
        _grid.water(tile)


func _prune(tile: Vector2i) -> void:
    if not _grid.has_tree(tile):
        return
    if await begin_work(PRUNE_SECONDS, "prune"):
        var cuttings := _grid.prune(tile)
        if cuttings > 0:
            inventory.add(&"compost", cuttings)


func _pick(tile: Vector2i) -> void:
    var crop := _grid.ripe_crop(tile)
    if crop == &"":
        return
    if await begin_work(PICK_SECONDS, "pick"):
        var picked := _grid.harvest(tile)
        _store_harvest(picked)


func _store_harvest(picked: Dictionary) -> void:
    var crop: StringName = picked.get("crop", &"")
    var amount: int = picked.get("amount", 0)
    var quality: int = picked.get("quality", ItemCatalog.Quality.NORMAL)
    if crop == &"" or amount <= 0:
        return
    var left := inventory.add(crop, amount, quality)
    if left > 0:
        _grid.drop_items(tile_position(), crop, left)
    harvested.emit(crop, amount - left)
    GameState.record_harvest(crop, amount - left)


func _plant(tile: Vector2i, seed_id: StringName) -> void:
    var item := ItemCatalog.find(seed_id)
    if item == null or not item.can_plant_in(GameState.season_flag()):
        GameState.notify("This seed will not grow in %s." % GameState.season_label())
        return
    if _grid.plant(tile, seed_id) and inventory.remove(seed_id, 1) == 1:
        GameState.notify("Planted %s." % item.display_name)


func _fertilize(tile: Vector2i, supply: StringName) -> void:
    if _grid.fertilize(tile, supply):
        inventory.remove(supply, 1)


func _pick_or_talk(tile: Vector2i) -> void:
    var neighbour := _grid.actor_at(tile)
    if neighbour != null and neighbour != self:
        GameState.start_dialogue(neighbour.display_name)
        return
    var dropped := _grid.pick_up(tile)
    for id in dropped:
        inventory.add(id, dropped[id])


func finish_work(label: String) -> void:
    match label:
        "water":
            GameState.add_experience(&"farming", 1)
        "prune":
            GameState.add_experience(&"orchard", 2)
        "pick":
            GameState.add_experience(&"orchard", 1)


## Buys `amount` of `id` if the farmer can pay and carry it.
func buy(id: StringName, amount: int = 1) -> bool:
    var cost := ItemCatalog.buy_price(id) * amount
    if cost <= 0 or cost > money:
        return false
    var left := inventory.add(id, amount)
    var bought := amount - left
    money -= ItemCatalog.buy_price(id) * bought
    return bought == amount


## Sells everything of one crop and returns the money earned.
func sell_all(id: StringName) -> int:
    var earned := 0
    for index in inventory.size:
        var slot := inventory.slot_at(index)
        if slot.id != id:
            continue
        earned += ItemCatalog.sell_price(id, slot.quality) * slot.amount
    var sold := inventory.remove(id, inventory.count(id))
    if sold > 0:
        money += earned
        GameState.record_sale(id, sold, earned)
    return earned


func craft(product: StringName) -> bool:
    var inputs := ItemCatalog.recipe_inputs(product)
    if inputs.is_empty() or not inventory.consume(inputs):
        return false
    var left := inventory.add(product, 1)
    if left > 0:
        _grid.drop_items(tile_position(), product, left)
    return true


func set_input_enabled(enabled: bool) -> void:
    _input_enabled = enabled
    if not enabled:
        stop()


func _on_day_started(day: int) -> void:
    stamina = max_stamina
    if day % 7 == 0:
        GameState.notify("Market day! Prices are higher at the stall.")


func _on_item_added(id: StringName, amount: int) -> void:
    if inventory.is_full():
        GameState.notify("Your basket is full.")
    elif amount > 10:
        GameState.notify("Collected %d %s." % [amount, ItemCatalog.find(id).display_name])


func _on_hurt_by(source: Node) -> void:
    if source is OrchardActor:
        GameState.notify("%s bumped into you." % source.display_name)
    else:
        GameState.notify("Ouch!")


func to_save_data() -> Dictionary:
    var data := super.to_save_data()
    data["money"] = money
    data["selected_slot"] = selected_slot
    data["inventory"] = inventory.to_save_data()
    return data


func load_save_data(data: Dictionary) -> void:
    super.load_save_data(data)
    money = int(data.get("money", START_MONEY))
    selected_slot = int(data.get("selected_slot", 0))
    inventory.load_save_data(data.get("inventory", []))
    select_slot(selected_slot)


func net_worth() -> int:
    return money + inventory.total_value()


func status_line() -> String:
    return "%s, %d coins, %d free slots" % [super.status_line(), money, inventory.free_slots()]


func hotbar_labels() -> Array[String]:
    var labels: Array[String] = []
    for slot in inventory.hotbar():
        if slot.is_empty():
            labels.append("-")
        else:
            labels.append("%s x%d" % [ItemCatalog.find(slot.id).display_name, slot.amount])
    return labels


func can_afford(id: StringName, amount: int = 1) -> bool:
    return ItemCatalog.buy_price(id) * amount <= money


func drop_selected() -> void:
    var item := selected_item()
    if item == &"" or _grid == null:
        return
    var removed := inventory.remove(item, 1)
    if removed > 0:
        _grid.drop_items(tile_in_front(), item, removed)


func eat(id: StringName) -> bool:
    var item := ItemCatalog.find(id)
    if item == null or item.category != ItemCatalog.Category.CROP:
        return false
    if inventory.remove(id, 1) == 0:
        return false
    heal(maxi(1, item.base_price / 5))
    stamina += item.base_price / 2.0
    return true
