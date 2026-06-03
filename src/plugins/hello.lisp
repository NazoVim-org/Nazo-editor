;; ijevim Lisp plugin example
;; API: (add-command name), (on event), (log msg ...)

(add-command "hello-lisp")
(on "Ready")
(log "hello-lisp: plugin loaded")
