extends Node
## Autoloaded as `GameState` from project.godot.

signal run_ended(score: int)

const SCORE_URL := "http://localhost:8080/api/scores"

var entities: Array[Entity] = []
var hits := 0
var http: HTTPRequest


func _ready() -> void:
    http = HTTPRequest.new()
    add_child(http)


func ping() -> void:
    var probe := HTTPRequest.new()
    add_child(probe)
    probe.request("http://localhost:8080/api/health")


func register(entity: Entity) -> void:
    entities.append(entity)
    entity.died.connect(_on_died)


func record_hit(amount: int) -> void:
    hits += amount


func end_run(hero: Hero) -> void:
    run_ended.emit(hits)
    http.request(SCORE_URL, [], HTTPClient.METHOD_POST, JSON.stringify({"hits": hits}))
    hero.queue_free()


func living() -> int:
    return entities.filter(func(entity): return entity.is_alive()).size()


func _on_died(entity: Entity) -> void:
    entities.erase(entity)
    print("Entity count: %d" % Entity.count())
