## Maps the item in the farmer's hand to what using it does.
extends RefCounted

enum Kind { NONE, WATER, PRUNE, PICK, PLANT, FERTILIZE }

const TOOL_KINDS := {
    &"watering_can": Kind.WATER,
    &"pruning_shears": Kind.PRUNE,
    &"basket": Kind.PICK,
}


static func for_item(id: StringName) -> Kind:
    if id == &"":
        return Kind.NONE
    if TOOL_KINDS.has(id):
        return TOOL_KINDS[id]
    var item := ItemCatalog.find(id)
    if item == null:
        return Kind.NONE
    match item.category:
        ItemCatalog.Category.SEED:
            return Kind.PLANT
        ItemCatalog.Category.FERTILIZER:
            return Kind.FERTILIZE
        _:
            return Kind.NONE


static func label(kind: Kind) -> String:
    return Kind.keys()[kind].capitalize()
