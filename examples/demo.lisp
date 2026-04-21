;;; Showcase for tulisp-ratatui. Demonstrates:
;;;   - multi-span list rows (per-column colouring)
;;;   - styled paragraph lines (per-span colouring inside a paragraph)
;;;   - #rrggbb hex colours
;;;   - highlight-bg (no REVERSED fallback)
;;;   - keyboard + mouse events (scroll, click)

(defvar demo/running  t)
(defvar demo/term     nil)
(defvar demo/selected 0)

;; Palette — pick any hex colours you like.
(setq demo/-palette
      '((fg     . "#f2fffc")
        (dim    . "#8b9798")
        (panel  . "#3a4449")
        (red    . "#ff6d7e")
        (orange . "#ffb270")
        (yellow . "#ffed72")
        (green  . "#a2e57b")
        (cyan   . "#7cd5f1")
        (violet . "#baa0f8")))

(defun demo/-c (name) (alist-get name demo/-palette))

;; Fake task list.
(setq demo/-tasks
      '(((id . 1) (priority . high)   (title . "ship the renderer")
         (progress . 0.85) (owner . "alice")
         (notes . "waiting on the shader review"))
        ((id . 2) (priority . medium) (title . "write documentation")
         (progress . 0.30) (owner . "bob")
         (notes . "cover every tui/* function"))
        ((id . 3) (priority . low)    (title . "investigate flaky test")
         (progress . 0.10) (owner . "carol")
         (notes . "only reproduces in CI on tuesdays"))
        ((id . 4) (priority . high)   (title . "cut 0.2 release")
         (progress . 0.60) (owner . "dan")
         (notes . "blocked on docs"))
        ((id . 5) (priority . medium) (title . "refactor the poll loop")
         (progress . 0.45) (owner . "erin")
         (notes . "250 ms timeout is plenty"))
        ((id . 6) (priority . low)    (title . "add more examples")
         (progress . 0.05) (owner . "frank")
         (notes . "this one counts"))))

(defun demo/-priority-colour (p)
  (cond
   ((eq p 'high)   (demo/-c 'red))
   ((eq p 'medium) (demo/-c 'orange))
   (t              (demo/-c 'green))))

;; One row = a list of styled spans. Columns: id (dim), priority (coloured
;; by severity), title (main fg). tui/list draws each span with its own
;; style, so the columns stay aligned and readable.
(defun demo/-task-row (task)
  (let* ((id    (alist-get 'id       task))
         (pri   (alist-get 'priority task))
         (title (alist-get 'title    task)))
    (list (cons (format "%3d  " id)
                `((fg . ,(demo/-c 'dim))))
          (cons (format "%-7s " (format "%s" pri))
                `((fg . ,(demo/-priority-colour pri)) (modifier . bold)))
          (cons (format "%s" title)
                `((fg . ,(demo/-c 'fg)))))))

;; One detail line = (LABEL-SPAN VALUE-SPAN). Paragraphs accept a list of
;; such lines and render each with its own spans.
(defun demo/-detail-line (label value-style value)
  (list (cons (format "%-9s " label)
              `((fg . ,(demo/-c 'dim))))
        (cons (format "%s" value) value-style)))

(defun demo/-task-detail-lines (task)
  (if (null task)
      (list (list (cons "(nothing selected)"
                        `((fg . ,(demo/-c 'dim)) (modifier . italic)))))
    (let* ((pri (alist-get 'priority task))
           (pri-style `((fg . ,(demo/-priority-colour pri)) (modifier . bold)))
           (fg-style  `((fg . ,(demo/-c 'fg)) (modifier . bold)))
           (val-style `((fg . ,(demo/-c 'cyan))))
           (num-style `((fg . ,(demo/-c 'yellow)))))
      (list (demo/-detail-line "id"       num-style (alist-get 'id task))
            (demo/-detail-line "title"    fg-style  (alist-get 'title task))
            (demo/-detail-line "priority" pri-style pri)
            (demo/-detail-line "owner"    val-style (alist-get 'owner task))
            (demo/-detail-line "progress" num-style
                               (format "%d%%" (floor (* 100 (alist-get 'progress task)))))
            (demo/-detail-line "notes"    val-style (alist-get 'notes task))))))

(defun demo/-header-lines ()
  (list (list (cons "↑/↓ "       `((fg . ,(demo/-c 'cyan)) (modifier . bold)))
              (cons "navigate   " `((fg . ,(demo/-c 'dim))))
              (cons "scroll/click " `((fg . ,(demo/-c 'cyan)) (modifier . bold)))
              (cons "list   "    `((fg . ,(demo/-c 'dim))))
              (cons "q "         `((fg . ,(demo/-c 'red)) (modifier . bold)))
              (cons "quit"       `((fg . ,(demo/-c 'dim)))))))

;; Mouse hit-testing: remember where we drew the list so clicks resolve
;; to indices.
(setq demo/-list-y 0)
(setq demo/-list-h 0)

(defun demo/-render ()
  (let* ((size     (tui/size demo/term))
         (cols     (car size))
         (rows     (cdr size))
         (header-h 3)
         (gauge-h  3)
         (body-y   header-h)
         (body-h   (- rows header-h gauge-h))
         (gauge-y  (+ body-y body-h))
         (left     (max 30 (floor (/ cols 2))))
         (right    (- cols left))
         (tasks    demo/-tasks)
         (count    (length tasks))
         (idx      (max 0 (min demo/selected (- count 1))))
         (current  (nth idx tasks))
         (items    (mapcar 'demo/-task-row tasks))
         (ratio    (alist-get 'progress current))
         (list-style
          `((border-fg       . ,(demo/-c 'cyan))
            (title-fg        . ,(demo/-c 'cyan)) (title-modifier . bold)
            ;; Subtle bg bar instead of the REVERSED fallback — the per-span
            ;; fg colours stay untouched, so the highlighted row keeps its
            ;; priority/title colouring.
            (highlight-bg    . ,(demo/-c 'panel))
            (highlight-modifier . bold))))
    (setq demo/selected idx)
    (setq demo/-list-y (+ body-y 1))
    (setq demo/-list-h (max 0 (- body-h 2)))
    (list
     (tui/paragraph 0    0       cols  header-h "tulisp-ratatui"
                    (demo/-header-lines)
                    `((border-fg . ,(demo/-c 'cyan))
                      (title-fg  . ,(demo/-c 'cyan)) (title-modifier . bold)))
     (tui/list      0    body-y  left  body-h "tasks" items idx list-style)
     (tui/paragraph left body-y  right body-h "details"
                    (demo/-task-detail-lines current)
                    `((fg        . ,(demo/-c 'fg))
                      (border-fg . ,(demo/-c 'violet))
                      (title-fg  . ,(demo/-c 'violet)) (title-modifier . bold)))
     (tui/gauge     0    gauge-y cols  gauge-h "progress" ratio
                    (format "%d%%" (floor (* 100 ratio)))
                    `((fg        . ,(demo/-c 'green))
                      (bg        . ,(demo/-c 'panel))
                      (border-fg . ,(demo/-c 'green))
                      (title-fg  . ,(demo/-c 'green)) (title-modifier . bold))))))

(defun demo/-handle-mouse (kind x y)
  (cond
   ((eq kind 'mouse-scroll-up)
    (setq demo/selected (max 0 (- demo/selected 1))))
   ((eq kind 'mouse-scroll-down)
    (setq demo/selected (+ demo/selected 1)))
   ((and (eq kind 'mouse-left)
         (>= y demo/-list-y)
         (< y (+ demo/-list-y demo/-list-h)))
    (setq demo/selected (- y demo/-list-y)))))

(defun demo/-handle (ev)
  (cond
   ((listp ev) (demo/-handle-mouse (car ev) (car (cdr ev)) (car (cdr (cdr ev)))))
   ((or (eq ev 'char-q) (eq ev 'C-char-c)) (setq demo/running nil))
   ((or (eq ev 'up)       (eq ev 'C-char-p)) (setq demo/selected (- demo/selected 1)))
   ((or (eq ev 'down)     (eq ev 'C-char-n)) (setq demo/selected (+ demo/selected 1)))
   ((or (eq ev 'page-up)  (eq ev 'M-char-v)) (setq demo/selected (- demo/selected 5)))
   ((or (eq ev 'page-down)(eq ev 'C-char-v)) (setq demo/selected (+ demo/selected 5)))
   ((eq ev 'home)                             (setq demo/selected 0))
   ((eq ev 'end)                              (setq demo/selected 999999))))

(setq demo/term (tui/init))
(while demo/running
  (tui/draw demo/term (demo/-render))
  (let ((ev (tui/poll-event 250)))
    (when ev (demo/-handle ev))))
(tui/restore)
