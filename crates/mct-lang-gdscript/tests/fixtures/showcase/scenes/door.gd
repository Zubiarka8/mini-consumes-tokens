extends Area2D
## Attached to the Door node in dungeon.tscn; nothing in code names it.

signal opened


func _on_body_entered(body: Node2D) -> void:
    if body is Hero:
        opened.emit()
