local user = require("app.models.user")
local util = require "lib.util"
local json = require("cjson")

print(user, util, json)
local shapes = require("app.shapes")
local legacy = dofile("lib/util.lua")
local loaded = loadfile("lib/user.lua")
