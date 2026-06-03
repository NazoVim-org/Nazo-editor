// ijevim JavaScript plugin example
// API: ijevim.addCommand(name, func), ijevim.on(event, handler), ijevim.log(msg)

ijevim.addCommand("hello-js", function() {
    ijevim.log("Hello JS command called!");
});

ijevim.on("ready", function() {
    ijevim.log("JS plugin: editor is ready!");
});

ijevim.log("Hello from JS plugin!");
