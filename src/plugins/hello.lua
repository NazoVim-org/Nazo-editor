-- ijevim Lua plugin example
-- API: addCommand(name, fn), on(event, fn), log(msg)

return {
  name = "hello-lua",
  version = "0.1.0",

  setup = function(api)
    api.addCommand("hello-lua", function()
      api.log("Hello from Lua plugin!")
    end)

    api.on("Ready", function()
      api.log("hello-lua: editor is ready")
    end)
  end
}
