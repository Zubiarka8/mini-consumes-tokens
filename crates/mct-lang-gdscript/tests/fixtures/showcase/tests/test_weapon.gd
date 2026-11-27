extends GutTest
## GUT tests: the file name and the `test_` methods mark them as tests.


func test_strike_adds_power() -> void:
    var sword := Weapon.create("sword", 7)
    assert_eq(sword.strike(3), 10)


func test_damage_never_drops_below_the_minimum() -> void:
    var stick := Weapon.new("stick", -5)
    assert_eq(stick.damage, Weapon.MIN_DAMAGE)
