--!nonstrict
-- Settings' pages but Providers (screens/8) from a mocked host: they show
-- `settings` as the host publishes it and ask the host for each change.

local function last_call(app)
	local c = app:state("calls")
	return c[#c]
end

local function settings(over)
	local s = {
		my_name = "Tom",
		theme = "system",
		autostart = false,
		language = "auto",
		caption_my_speech = false,
		caption_size = 20,
		caption_lines = 2,
		partial_text = true,
		hide_from_share = true,
		microphone = "",
		meeting_audio = "system",
		suggest_on_call = true,
		glossary = { "Priya", "David", "payments API", "dark mode", "load testing", "soft launch", "release notes" },
		hotkeys = { start_notes = "Ctrl+Alt+N", pause = "Ctrl+Alt+P", click_through = "Ctrl+Alt+C", captions = "Ctrl+Alt+O" },
		storage_folder = "",
		keep_audio_days = 0,
	}
	for k, v in over or {} do
		s[k] = v
	end
	return s
end

-- Settings open on `page`.
local function open(app, page, over)
	app:set_state("settings", settings(over))
	app:set_state("settings_page", page)
	app:set_state("window.settings", true)
	app:frames(2)
	return app:window("settings")
end

test("General shows screens/8: you, captions, audio and the glossary", function()
	local win = open(mount(), "general")
	for _, title in { "You", "Captions", "Audio", "Glossary" } do
		expect(win:has(`"{title}"`)).to_be(true)
	end
	expect(win:has('"My name"')).to_be(true)
	expect(win:text("#set-theme-label")).to_be("Follow system")
	expect(win:text("#set-caption_size-value")).to_be("20")
	expect(win:text("#set-caption_my_speech-state")).to_be("off")
	expect(win:text("#set-partial_text-state")).to_be("on")
	expect(win:text("#set-hide_from_share-state")).to_be("on")
	expect(win:text("#set-microphone-label")).to_be("Default")
	expect(win:text("#set-meeting_audio-label")).to_be("All system audio")
	expect(win:text("#set-suggest_on_call-state")).to_be("on")
	expect(win:text("#term-1")).to_be("Priya")
	expect(win:text("#term-7")).to_be("release notes")
	expect(win:has("#glossary-input")).to_be(true)
end)

-- A call mixing Vietnamese and English came out translated while the
-- engines were told English (ADR 0022).
test("General sets the meeting language: auto-detected unless chosen", function()
	local app = mount()
	local win = open(app, "general")
	expect(win:text("#set-language-label")).to_be("Auto-detect")
	local win2 = open(mount(), "general", { language = "vi" })
	expect(win2:text("#set-language-label")).to_be("Tiếng Việt")
	-- A code set by hand shows as itself.
	local win3 = open(mount(), "general", { language = "ja" })
	expect(win3:text("#set-language-label")).to_be("ja")
end)

test("a switch asks the host to flip its setting", function()
	local app = mount()
	local win = open(app, "captions")
	win:click("#set-partial_text")
	expect(last_call(app)).to_equal({ name = "set_settings", arg = { partial_text = false } })
	win:click("#set-caption_my_speech")
	expect(last_call(app)).to_equal({ name = "set_settings", arg = { caption_my_speech = true } })
end)

test("caption size and lines step within their range", function()
	local app = mount()
	local win = open(app, "captions", { caption_size = 40, caption_lines = 1 })
	win:click("#set-caption_size-minus")
	expect(last_call(app)).to_equal({ name = "set_settings", arg = { caption_size = 38 } })
	local before = #app:state("calls")
	win:click("#set-caption_size-plus")
	expect(#app:state("calls")).to_be(before) -- 40 is the largest
	win:click("#set-caption_lines-plus")
	expect(last_call(app)).to_equal({ name = "set_settings", arg = { caption_lines = 2 } })
end)

test("the theme menu offers system, dark and light", function()
	local app = mount()
	local win = open(app, "general")
	win:click("#set-theme")
	expect(win:wait("#set-theme-light")).to_be(true)
	expect(win:has("#set-theme-dark")).to_be(true)
	win:click("#set-theme-light")
	expect(last_call(app)).to_equal({ name = "set_settings", arg = { theme = "light" } })
end)

test("the theme shows what is saved", function()
	local win = open(mount(), "general", { theme = "dark" })
	expect(win:text("#set-theme-label")).to_be("Dark")
end)

test("Audio lists the microphones and asks for them again", function()
	local app = mount()
	app:set_state("microphones", { { id = "{headset}", name = "Headset Mic" }, { id = "{array}", name = "Laptop Mic" } })
	local win = open(app, "audio", { microphone = "{headset}" })
	expect(win:text("#set-microphone-label")).to_be("Headset Mic")
	local asked = false
	for _, c in app:state("calls") do
		asked = asked or c.name == "refresh_microphones"
	end
	expect(asked).to_be(true)
	win:click("#set-microphone")
	expect(win:wait("#set-microphone-{array}")).to_be(true)
	win:click("#set-microphone-{array}")
	expect(last_call(app)).to_equal({ name = "set_settings", arg = { microphone = "{array}" } })
end)

test("meeting audio can be one app", function()
	local app = mount()
	local win = open(app, "audio")
	win:click("#set-meeting_audio")
	expect(win:wait("#set-meeting_audio-ms-teams.exe")).to_be(true)
	win:click("#set-meeting_audio-ms-teams.exe")
	expect(last_call(app)).to_equal({ name = "set_settings", arg = { meeting_audio = "ms-teams.exe" } })
end)

test("a glossary term is added with Enter and removed with its x", function()
	local app = mount()
	local win = open(app, "glossary")
	win:type("#glossary-input", "soft launch")
	win:key("Enter")
	expect(last_call(app)).to_equal({ name = "add_term", arg = "soft launch" })
	expect(app:state("glossary_draft")).to_be("")
	win:click("#term-2-remove")
	expect(last_call(app)).to_equal({ name = "remove_term", arg = "David" })
end)

test("a refused term says why", function()
	local app = mount()
	local win = open(app, "glossary")
	expect(win:has("#glossary-error")).to_be(false)
	app:set_state("settings_error", { field = "glossary", message = "Up to 20 characters" })
	expect(win:wait("#glossary-error")).to_be(true)
	expect(win:text("#glossary-error")).to_be("Up to 20 characters")
end)

test("a full glossary has no Add", function()
	local terms = {}
	for i = 1, 50 do
		table.insert(terms, `term {i}`)
	end
	local win = open(mount(), "glossary", { glossary = terms })
	expect(win:has("#term-50")).to_be(true)
	expect(win:has("#glossary-input")).to_be(false)
end)

test("Shortcuts shows each hotkey as keycaps", function()
	local win = open(mount(), "shortcuts")
	expect(win:text("#hotkey-start_notes-keys")).to_be("Ctrl+Alt+N")
	expect(win:text("#hotkey-pause-keys")).to_be("Ctrl+Alt+P")
	expect(win:has('"Start notes"')).to_be(true)
end)

test("a hotkey is recorded from the next keys pressed", function()
	local app = mount()
	local win = open(app, "shortcuts")
	win:click("#hotkey-pause")
	expect(win:wait("#hotkey-pause-recording")).to_be(true)
	win:key("ctrl+shift+k")
	expect(last_call(app)).to_equal({ name = "set_hotkey", arg = { id = "pause", keys = "Ctrl+Shift+K" } })
	expect(app:state("hotkey_recording")).to_be("")
end)

test("Esc stops recording a hotkey and keeps it", function()
	local app = mount()
	local win = open(app, "shortcuts")
	win:click("#hotkey-captions")
	win:key("Escape")
	expect(app:state("hotkey_recording")).to_be("")
	expect(last_call(app)).to_be_nil()
end)

test("a refused hotkey says why under the card", function()
	local app = mount()
	local win = open(app, "shortcuts")
	app:set_state("settings_error", { field = "hotkey.captions", message = "Used by Pause" })
	expect(win:wait("#hotkey-captions-error")).to_be(true)
	expect(win:text("#hotkey-captions-error")).to_be("Used by Pause")
end)

test("a hotkey another app holds is reported under the card", function()
	local app = mount()
	local win = open(app, "shortcuts")
	expect(win:has("#hotkey-captions-taken")).to_be(false)
	app:set_state("hotkey_conflicts", { captions = "Another app uses Ctrl+Alt+O" })
	expect(win:wait("#hotkey-captions-taken")).to_be(true)
	expect(win:text("#hotkey-captions-taken")).to_be("Another app uses Ctrl+Alt+O")
	expect(win:has("#hotkey-pause-taken")).to_be(false)
end)

test("recording a hotkey is shared, so the host can rest the global ones", function()
	local app = mount()
	local win = open(app, "shortcuts")
	win:click("#hotkey-start_notes")
	expect(app:state("hotkey_recording")).to_be("start_notes")
	win:key("Escape")
	expect(app:state("hotkey_recording")).to_be("")
end)

test("Storage shows the folder and changes it with a dialog", function()
	local app = mount()
	local win = open(app, "storage", { storage_folder = "D:\\Meetings" })
	expect(win:text("#storage-folder")).to_be("D:\\Meetings")
	win:click("#storage-change")
	expect(last_call(app).name).to_be("choose_storage_folder")
end)

test("audio retention is picked from a menu", function()
	local app = mount()
	local win = open(app, "storage")
	expect(win:text("#set-keep_audio_days-label")).to_be("Forever")
	win:click("#set-keep_audio_days")
	expect(win:wait("#set-keep_audio_days-30")).to_be(true)
	win:click("#set-keep_audio_days-30")
	expect(last_call(app)).to_equal({ name = "set_settings", arg = { keep_audio_days = 30 } })
end)
