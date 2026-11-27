class_name Torch
extends PointLight2D
## Attached in dungeon.tscn and named nowhere in code: the scene keeps it alive.

@export var flicker_speed := 2.0


func _process(delta: float) -> void:
    energy = 1.0 + sin(Time.get_ticks_msec() * flicker_speed * delta)
