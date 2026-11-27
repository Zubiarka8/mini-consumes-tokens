class_name WorkshopPlayer
extends "res://scripts/base.gd"

const Helper = preload("res://scripts/helper.gd")
var client: HTTPRequest

func _init():
    super()

func play(steps: int = 2) -> void:
    self.move(steps)
    Helper.new()
    client.request("http://localhost:8080/api/workshop")

func move(steps: int) -> void:
    print("Workshop player movement", steps)
