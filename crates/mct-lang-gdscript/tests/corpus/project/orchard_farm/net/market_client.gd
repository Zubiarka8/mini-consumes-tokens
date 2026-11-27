## Talks to the village market server: daily prices, selling crates and the
## weekly leaderboard. Requests go through one typed `HTTPRequest`, so the
## literal URLs below are HTTP endpoints this script uses.
class_name MarketClient
extends Node

signal prices_updated(changed: int)
signal crate_sold(id: StringName, amount: int, earned: int)
signal leaderboard_received(rows: Array)
signal request_failed(reason: String)

const TIMEOUT_SECONDS := 8.0
const RETRY_LIMIT := 3

enum Pending { NONE, PRICES, SELL, LEADERBOARD, PING }

var client: HTTPRequest
var farmer: Farmer
var _pending := Pending.NONE
var _queue: Array[Dictionary] = []
var _retries := 0
var _last_prices: Dictionary = {}


func _ready() -> void:
    client = HTTPRequest.new()
    client.timeout = TIMEOUT_SECONDS
    add_child(client)
    client.request_completed.connect(_on_request_completed)
    GameState.day_started.connect(_on_day_started)


func _on_day_started(_day: int) -> void:
    fetch_prices()


func is_busy() -> bool:
    return _pending != Pending.NONE


## Asks the server for today's prices.
func fetch_prices() -> void:
    if is_busy():
        _queue.append({"kind": Pending.PRICES})
        return
    _pending = Pending.PRICES
    var error := client.request("http://localhost:8080/api/market/prices")
    _check_started(error)


## Sells a crate of one crop at the market stall.
func sell_crate(id: StringName, amount: int) -> void:
    if farmer == null or farmer.inventory.count(id) < amount:
        request_failed.emit("Not enough %s to sell." % id)
        return
    if is_busy():
        _queue.append({"kind": Pending.SELL, "id": id, "amount": amount})
        return
    _pending = Pending.SELL
    var body := JSON.stringify({"item": String(id), "amount": amount, "day": GameState.day})
    var headers := PackedStringArray(["Content-Type: application/json"])
    var error := client.request("http://localhost:8080/api/market/sell", headers, HTTPClient.METHOD_POST, body)
    _check_started(error)
    farmer.inventory.locked = true


func fetch_leaderboard() -> void:
    if is_busy():
        _queue.append({"kind": Pending.LEADERBOARD})
        return
    _pending = Pending.LEADERBOARD
    var error := client.request("http://localhost:8080/api/market/leaderboard", [], HTTPClient.METHOD_GET)
    _check_started(error)


## Removes this farm from the leaderboard when the player starts over.
func leave_leaderboard(farm_id: String) -> void:
    if is_busy():
        return
    _pending = Pending.LEADERBOARD
    var error := client.request("http://localhost:8080/api/market/leaderboard/" + farm_id, [], HTTPClient.METHOD_DELETE)
    _check_started(error)


func ping() -> void:
    if is_busy():
        return
    _pending = Pending.PING
    _check_started(client.request("http://localhost:8080/api/market/health", [], HTTPClient.METHOD_HEAD))


func _check_started(error: Error) -> void:
    if error == OK:
        return
    var kind := _pending
    _pending = Pending.NONE
    if farmer != null:
        farmer.inventory.locked = false
    request_failed.emit("Could not reach the market (%s)." % error_string(error))
    _retry_later(kind)


func _retry_later(kind: Pending) -> void:
    if _retries >= RETRY_LIMIT or kind == Pending.PING:
        _retries = 0
        return
    _retries += 1
    await get_tree().create_timer(2.0 * _retries).timeout
    _queue.push_front({"kind": kind})
    _next()


func _on_request_completed(result: int, code: int, _headers: PackedStringArray, body: PackedByteArray) -> void:
    var kind := _pending
    _pending = Pending.NONE
    if farmer != null:
        farmer.inventory.locked = false
    if result != HTTPRequest.RESULT_SUCCESS or code >= 400:
        request_failed.emit("The market answered %d." % code)
        _retry_later(kind)
        return
    _retries = 0
    var payload = JSON.parse_string(body.get_string_from_utf8())
    match kind:
        Pending.PRICES:
            _apply_prices(payload)
        Pending.SELL:
            _apply_sale(payload)
        Pending.LEADERBOARD:
            _apply_leaderboard(payload)
        Pending.PING:
            pass
    _next()


func _next() -> void:
    if _queue.is_empty() or is_busy():
        return
    var job: Dictionary = _queue.pop_front()
    match job["kind"]:
        Pending.PRICES:
            fetch_prices()
        Pending.SELL:
            sell_crate(job["id"], job["amount"])
        Pending.LEADERBOARD:
            fetch_leaderboard()


func _apply_prices(payload) -> void:
    if not payload is Dictionary or not payload.has("prices"):
        request_failed.emit("The price list was empty.")
        return
    var prices: Dictionary = payload["prices"]
    if GameState.is_market_day():
        for id in prices:
            prices[id] = int(prices[id] * GameState.price_bonus())
    _last_prices = prices.duplicate()
    var changed := ItemCatalog.apply_price_update(prices)
    prices_updated.emit(changed)
    if changed > 0:
        GameState.notify("Market prices changed for %d items." % changed)


func _apply_sale(payload) -> void:
    if not payload is Dictionary:
        return
    var id := StringName(payload.get("item", ""))
    var amount := int(payload.get("amount", 0))
    var earned := int(payload.get("earned", 0))
    if id == &"" or amount <= 0:
        return
    var removed := farmer.inventory.remove(id, amount)
    if removed < amount:
        push_warning("market sold %d %s but only %d were in the basket" % [amount, id, removed])
    farmer.money += earned
    GameState.record_sale(id, removed, earned)
    crate_sold.emit(id, removed, earned)


func _apply_leaderboard(payload) -> void:
    var rows: Array = []
    if payload is Dictionary:
        rows = payload.get("rows", [])
    elif payload is Array:
        rows = payload
    rows.sort_custom(func(a, b): return a.get("score", 0) > b.get("score", 0))
    leaderboard_received.emit(rows)


func last_price(id: StringName) -> int:
    return int(_last_prices.get(String(id), ItemCatalog.sell_price(id)))


func price_trend(id: StringName) -> String:
    var previous := ItemCatalog.sell_price(id)
    var latest := last_price(id)
    if latest > previous:
        return "rising"
    if latest < previous:
        return "falling"
    return "steady"


func best_crop_to_sell() -> StringName:
    if farmer == null:
        return &""
    var best: StringName = &""
    var best_value := 0
    for item in ItemCatalog.in_category(ItemCatalog.Category.CROP):
        var held := farmer.inventory.count(item.id)
        var value := held * last_price(item.id)
        if value > best_value:
            best = item.id
            best_value = value
    return best


func queued_jobs() -> int:
    return _queue.size()


func cancel_all() -> void:
    _queue.clear()
    if is_busy():
        client.cancel_request()
        _pending = Pending.NONE
    if farmer != null:
        farmer.inventory.locked = false


func pending_label() -> String:
    match _pending:
        Pending.PRICES:
            return "Checking prices..."
        Pending.SELL:
            return "Selling a crate..."
        Pending.LEADERBOARD:
            return "Loading the leaderboard..."
        Pending.PING:
            return "Looking for the market..."
    return ""


func describe_prices() -> String:
    var lines: Array[String] = []
    for item in ItemCatalog.in_category(ItemCatalog.Category.CROP):
        lines.append("%s: %d (%s)" % [item.display_name, last_price(item.id), price_trend(item.id)])
    return "\n".join(lines)


func sell_everything() -> void:
    if farmer == null:
        return
    for item in ItemCatalog.in_category(ItemCatalog.Category.CROP):
        var held := farmer.inventory.count(item.id)
        if held > 0:
            sell_crate(item.id, held)


func expected_earnings(id: StringName, amount: int) -> int:
    return last_price(id) * amount


func farm_score() -> int:
    if farmer == null:
        return 0
    var score := farmer.net_worth()
    for crop in GameState.harvest_totals:
        score += int(GameState.harvest_totals[crop]) * 2
    return score


func submit_score(farm_id: String) -> void:
    if is_busy():
        return
    _pending = Pending.LEADERBOARD
    var body := JSON.stringify({"farm": farm_id, "score": farm_score(), "day": GameState.day})
    var error := client.request("http://localhost:8080/api/market/leaderboard", ["Content-Type: application/json"], HTTPClient.METHOD_PUT, body)
    _check_started(error)


func stats() -> Dictionary:
    return {
        "pending": Pending.keys()[_pending],
        "queued": _queue.size(),
        "retries": _retries,
        "known_prices": _last_prices.size(),
    }
