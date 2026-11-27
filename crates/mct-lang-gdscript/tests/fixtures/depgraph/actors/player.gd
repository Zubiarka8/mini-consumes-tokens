extends "res://core/base.gd"

const Weapon = preload("../items/weapon.gd")
const Missing = preload("res://items/missing.gd")


func equip():
    var shield = load("res://items/shield.gd")
    return [Weapon.new(), shield]
