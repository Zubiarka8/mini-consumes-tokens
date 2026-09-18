local Logger = {}

function Logger.log(message)
  print("[LOG] " .. message)
end

function Logger.warn(message)
  Logger.log("WARNING: " .. message)
end

return Logger
