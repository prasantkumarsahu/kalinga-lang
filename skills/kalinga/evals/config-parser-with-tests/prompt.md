---
tags: [skill-creator]
max_turns: 20
timeout_seconds: 600
allowed_tools: [Read, Glob, Grep, Skill, Write, Edit, Bash]
---

I need a Kalinga function that parses config text like "name=Asha\nport=8080\n# comment\n" into a map from keys to values. Blank lines and lines starting with # are skipped; a line without = should be an error that says which line number it was. Also add a helper that reads a key as an int and errors if it's missing or not a number. Put it in config.kl with test fns covering the good and the bad cases.
