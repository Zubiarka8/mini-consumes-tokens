## The orchard field: a grid of tiles holding soil, trees, crops and dropped
## items. Growth advances once per day from `GameState`.
class_name FarmGrid
extends Node2D

const Weather = preload("./weather.gd")

signal tile_changed(tile: Vector2i)
signal crop_ripened(tile: Vector2i, crop: StringName)

const TILE_SIZE := 16
const WATER_DAYS := 1
const MAX_FERTILITY := 3

enum Soil { GRASS, TILLED, WATERED, ROCK }


## Everything the grid knows about one tile.
class Tile:
    var soil := Soil.GRASS
    var seed_id: StringName = &""
    var days_grown := 0
    var watered_days := 0
    var fertility := 0
    var is_tree := false
    var pruned_today := false
    var fruit := 0
    var dropped: Dictionary = {}

    func has_crop() -> bool:
        return seed_id != &""

    func clear_crop() -> void:
        seed_id = &""
        days_grown = 0
        is_tree = false
        fruit = 0

    func to_dict() -> Dictionary:
        return {
            "soil": soil,
            "seed": String(seed_id),
            "days": days_grown,
            "watered": watered_days,
            "fertility": fertility,
            "tree": is_tree,
            "fruit": fruit,
        }

    static func from_dict(data: Dictionary) -> Tile:
        var tile := Tile.new()
        tile.soil = int(data.get("soil", Soil.GRASS))
        tile.seed_id = StringName(data.get("seed", ""))
        tile.days_grown = int(data.get("days", 0))
        tile.watered_days = int(data.get("watered", 0))
        tile.fertility = int(data.get("fertility", 0))
        tile.is_tree = bool(data.get("tree", false))
        tile.fruit = int(data.get("fruit", 0))
        return tile


@export var width := 32
@export var height := 24

var _tiles: Dictionary = {}
var _actors: Array[OrchardActor] = []
var _weather: Weather


func _ready() -> void:
    _weather = Weather.new()
    add_child(_weather)
    GameState.day_started.connect(advance_day)
    _weather.rain_started.connect(_on_rain_started)


func _tile(tile: Vector2i) -> Tile:
    if not _in_bounds(tile):
        return null
    if not _tiles.has(tile):
        _tiles[tile] = Tile.new()
    return _tiles[tile]


func _in_bounds(tile: Vector2i) -> bool:
    return tile.x >= 0 and tile.y >= 0 and tile.x < width and tile.y < height


func register_actor(actor: OrchardActor) -> void:
    if not _actors.has(actor):
        _actors.append(actor)


func actor_at(tile: Vector2i) -> OrchardActor:
    for actor in _actors:
        if actor.tile_position(TILE_SIZE) == tile:
            return actor
    return null


func till(tile: Vector2i) -> bool:
    var cell := _tile(tile)
    if cell == null or cell.soil != Soil.GRASS:
        return false
    cell.soil = Soil.TILLED
    tile_changed.emit(tile)
    return true


func can_water(tile: Vector2i) -> bool:
    var cell := _tile(tile)
    return cell != null and cell.soil in [Soil.TILLED, Soil.WATERED]


func water(tile: Vector2i) -> void:
    var cell := _tile(tile)
    if cell == null or not can_water(tile):
        return
    cell.soil = Soil.WATERED
    cell.watered_days = WATER_DAYS
    tile_changed.emit(tile)


func plant(tile: Vector2i, seed_id: StringName) -> bool:
    var cell := _tile(tile)
    if cell == null or cell.has_crop() or cell.soil == Soil.GRASS or cell.soil == Soil.ROCK:
        return false
    var item := ItemCatalog.find(seed_id)
    if item == null or not item.is_seed():
        return false
    cell.seed_id = seed_id
    cell.days_grown = 0
    cell.is_tree = item.days_to_grow >= 9
    tile_changed.emit(tile)
    return true


func fertilize(tile: Vector2i, supply: StringName) -> bool:
    var cell := _tile(tile)
    if cell == null or cell.fertility >= MAX_FERTILITY:
        return false
    match supply:
        &"compost":
            cell.fertility += 1
        &"bone_meal":
            cell.fertility = mini(cell.fertility + 2, MAX_FERTILITY)
        _:
            return false
    tile_changed.emit(tile)
    return true


func has_tree(tile: Vector2i) -> bool:
    var cell := _tile(tile)
    return cell != null and cell.is_tree


## Prunes a tree once a day. Returns the compost cuttings it gives.
func prune(tile: Vector2i) -> int:
    var cell := _tile(tile)
    if cell == null or not cell.is_tree or cell.pruned_today:
        return 0
    cell.pruned_today = true
    cell.fertility = mini(cell.fertility + 1, MAX_FERTILITY)
    tile_changed.emit(tile)
    return 1 + cell.days_grown / 10


func ripe_crop(tile: Vector2i) -> StringName:
    var cell := _tile(tile)
    if cell == null or not cell.has_crop():
        return &""
    var item := ItemCatalog.find(cell.seed_id)
    if item == null or cell.days_grown < item.days_to_grow:
        return &""
    if cell.is_tree and cell.fruit == 0:
        return &""
    return item.grows_into


## Picks what is ripe on `tile`: `{crop, amount, quality}`.
func harvest(tile: Vector2i) -> Dictionary:
    var crop := ripe_crop(tile)
    if crop == &"":
        return {}
    var cell := _tile(tile)
    var amount := cell.fruit if cell.is_tree else 1
    var quality := _quality_for(cell)
    if cell.is_tree:
        cell.fruit = 0
    else:
        cell.clear_crop()
        cell.soil = Soil.TILLED
    tile_changed.emit(tile)
    return {"crop": crop, "amount": amount, "quality": quality}


func _quality_for(cell: Tile) -> int:
    var score := cell.fertility + (1 if cell.pruned_today else 0)
    if _weather.is_drought():
        score -= 1
    if score >= 3:
        return ItemCatalog.Quality.PRIZE
    if score == 2:
        return ItemCatalog.Quality.GOOD
    if score < 0:
        return ItemCatalog.Quality.POOR
    return ItemCatalog.Quality.NORMAL


func drop_items(tile: Vector2i, id: StringName, amount: int) -> void:
    var cell := _tile(tile)
    if cell == null or amount <= 0:
        return
    cell.dropped[id] = cell.dropped.get(id, 0) + amount
    tile_changed.emit(tile)


func pick_up(tile: Vector2i) -> Dictionary:
    var cell := _tile(tile)
    if cell == null or cell.dropped.is_empty():
        return {}
    var items := cell.dropped
    cell.dropped = {}
    tile_changed.emit(tile)
    return items


## Advances every tile by one day. Unwatered crops do not grow.
func advance_day(_day: int) -> void:
    var season := GameState.season_flag()
    for position in _tiles:
        var cell: Tile = _tiles[position]
        cell.pruned_today = false
        if not cell.has_crop():
            _dry_out(cell)
            continue
        var item := ItemCatalog.find(cell.seed_id)
        if item == null:
            cell.clear_crop()
            continue
        if cell.watered_days > 0 or _weather.is_raining():
            cell.days_grown += 1
        if cell.is_tree and cell.days_grown >= item.days_to_grow and item.can_plant_in(season):
            cell.fruit = mini(cell.fruit + 1 + cell.fertility, 9)
            crop_ripened.emit(position, item.grows_into)
        elif not cell.is_tree and cell.days_grown == item.days_to_grow:
            crop_ripened.emit(position, item.grows_into)
        _dry_out(cell)
        tile_changed.emit(position)


func _dry_out(cell: Tile) -> void:
    if cell.watered_days > 0:
        cell.watered_days -= 1
    if cell.watered_days == 0 and cell.soil == Soil.WATERED:
        cell.soil = Soil.TILLED


func _on_rain_started() -> void:
    for position in _tiles:
        var cell: Tile = _tiles[position]
        if cell.soil == Soil.TILLED:
            cell.soil = Soil.WATERED
            cell.watered_days = WATER_DAYS
            tile_changed.emit(position)


func ripe_tiles() -> Array[Vector2i]:
    var result: Array[Vector2i] = []
    for position in _tiles:
        if ripe_crop(position) != &"":
            result.append(position)
    return result


func crop_counts() -> Dictionary:
    var counts := {}
    for position in _tiles:
        var cell: Tile = _tiles[position]
        if cell.has_crop():
            counts[cell.seed_id] = counts.get(cell.seed_id, 0) + 1
    return counts


func to_save_data() -> Dictionary:
    var rows := {}
    for position in _tiles:
        var cell: Tile = _tiles[position]
        rows["%d,%d" % [position.x, position.y]] = cell.to_dict()
    return {"width": width, "height": height, "tiles": rows}


func load_save_data(data: Dictionary) -> void:
    width = int(data.get("width", width))
    height = int(data.get("height", height))
    _tiles.clear()
    var rows: Dictionary = data.get("tiles", {})
    for key in rows:
        var parts: PackedStringArray = String(key).split(",")
        if parts.size() != 2:
            continue
        var position := Vector2i(int(parts[0]), int(parts[1]))
        _tiles[position] = Tile.from_dict(rows[key])
        tile_changed.emit(position)


func world_to_tile(point: Vector2) -> Vector2i:
    return Vector2i(floori(point.x / TILE_SIZE), floori(point.y / TILE_SIZE))


func tile_to_world(tile: Vector2i) -> Vector2:
    return Vector2(tile.x * TILE_SIZE + TILE_SIZE / 2.0, tile.y * TILE_SIZE + TILE_SIZE / 2.0)


func neighbours(tile: Vector2i) -> Array[Vector2i]:
    var result: Array[Vector2i] = []
    for offset in [Vector2i.UP, Vector2i.DOWN, Vector2i.LEFT, Vector2i.RIGHT]:
        var next: Vector2i = tile + offset
        if _in_bounds(next):
            result.append(next)
    return result


## Breaks a rock tile into tilled soil. Needs a full day of stamina.
func clear_rock(tile: Vector2i) -> bool:
    var cell := _tile(tile)
    if cell == null or cell.soil != Soil.ROCK:
        return false
    cell.soil = Soil.TILLED
    tile_changed.emit(tile)
    return true


func describe_tile(tile: Vector2i) -> String:
    var cell := _tile(tile)
    if cell == null:
        return "Out of bounds"
    var soil_name: String = Soil.keys()[cell.soil].capitalize()
    if not cell.has_crop():
        return soil_name
    var item := ItemCatalog.find(cell.seed_id)
    return "%s with %s, day %d of %d" % [soil_name, item.display_name, cell.days_grown, item.days_to_grow]
