extends CharacterBody2D


func _ready():
    ItemCatalog.find(&"seed")
    Game.start()
    Scores.submit(10)
    Input.is_action_pressed("jump")


func use(stock):
    stock.find(&"seed")
