# Changelog

Written from the conventional commits in each release.

## [0.2.1](https://github.com/ilbumi/timemd/compare/v0.2.0...v0.2.1) - 2026-08-04

### Fixed

- **schedule:** stop the day timeline cropping short block titles (#19) ([17b205a](https://github.com/ilbumi/timemd/commit/17b205a37fa9ddf5c2138d2e00bcc369566e4974))
## [0.2.0](https://github.com/ilbumi/timemd/compare/v0.1.0...v0.2.0) - 2026-08-03

### Added

- report what was planned beside what was tracked (#13) ([3aefb2d](https://github.com/ilbumi/timemd/commit/3aefb2d8a2e134a385b1b6b7924945d03b58fd60))
- **breaking:** close the operation-parity gaps across all four surfaces (#14) ([bc0d691](https://github.com/ilbumi/timemd/commit/bc0d6911b94616898f21574f45de8aafc107d97b))
- add ntfy as a second notification channel (#15) ([12acd47](https://github.com/ilbumi/timemd/commit/12acd470047adad051b649d779b4d076c452d4da))
- **release:** publish a container image to ghcr.io (#16) ([636251c](https://github.com/ilbumi/timemd/commit/636251c3b8e659a28c4c4ce895686395ccbfb2d0))
## [0.1.0](https://github.com/ilbumi/timemd/releases/tag/v0.1.0) - 2026-08-02

### Added

- **core:** add the markdown-file store ([a9bcfbf](https://github.com/ilbumi/timemd/commit/a9bcfbfe46ecfe14999793d468bd195984dca4b6))
- **server:** add project CRUD endpoints ([0d8c1b9](https://github.com/ilbumi/timemd/commit/0d8c1b92287368358ea5ef792e088210ec0dbf42))
- **server:** embed and serve the web UI ([4b5a9af](https://github.com/ilbumi/timemd/commit/4b5a9afc6a00ffa3e2905b4880dcdedc031b9b7c))
- **frontend:** add the app shell and projects screen ([cf09624](https://github.com/ilbumi/timemd/commit/cf0962498c998e7aab2a5086da0029f25382dcce))
- **core:** add the server-authoritative pomodoro timer ([8fefc98](https://github.com/ilbumi/timemd/commit/8fefc987eaf45b2b4286c0603016bc2da970537a))
- **server:** expose the timer and retire sessions in the background ([bb95070](https://github.com/ilbumi/timemd/commit/bb9507014efc48237798a615b888cbfd7848eec6))
- **frontend:** add the timer screen ([1d99668](https://github.com/ilbumi/timemd/commit/1d99668d15106d21568a571cebe7dda4d1e58ed4))
- **core:** add recurring and one-off schedule blocks ([4ed45bd](https://github.com/ilbumi/timemd/commit/4ed45bd314dcbfed48b584a7131349f7654782f1))
- **server:** expose the schedule and day editing ([ecebd56](https://github.com/ilbumi/timemd/commit/ecebd565fe3a1f14ddfd16cc036fb952e8224e8e))
- **frontend:** add the today and schedule screens ([ab6baf0](https://github.com/ilbumi/timemd/commit/ab6baf047e672e477669a99ff68e59f72574dbf3))
- add reports over a date range ([e230502](https://github.com/ilbumi/timemd/commit/e230502871366993223adf6b527e2350abe973ea))
- **cli:** add the shell operations agents reach for ([aa0ad0a](https://github.com/ilbumi/timemd/commit/aa0ad0ad738c2e4c486161c2122226e58d74bd8d))
- **core:** add reminder scheduling and push subscription state ([ebde7e7](https://github.com/ilbumi/timemd/commit/ebde7e72da1aa71c32e6d2d778e6413e061340b9))
- **server:** deliver reminders and session completions by web push ([8af5d3b](https://github.com/ilbumi/timemd/commit/8af5d3bb863ceada1d3cbc2f7b07073b79729936))
- **frontend:** add the PWA shell, service worker and settings screen ([ee388d5](https://github.com/ilbumi/timemd/commit/ee388d52a3f645ad8d0f7c5d512a2a3a1b2a3002))
- **mcp:** expose timemd to agents over the Model Context Protocol ([c33b448](https://github.com/ilbumi/timemd/commit/c33b448b8f109b3343b0ee2677c0c6ce79f579a8))
- **core:** give projects a mark, a weekly target and milestones ([66968bd](https://github.com/ilbumi/timemd/commit/66968bd50c3183d9fa67641d654da61be056ad65))
- **server:** expose marks, targets and milestones over HTTP, MCP and the CLI ([45d14ce](https://github.com/ilbumi/timemd/commit/45d14cee55ebab2b88c854e4d66b67a8bc091f5f))
- **server:** expose the pomodoro lengths at /api/settings ([c6e98dd](https://github.com/ilbumi/timemd/commit/c6e98dd5dc3103f39edd5ef30a6ced4cd7861d83))
- **frontend:** rebuild the app in the Bauhaus design language ([9b773d7](https://github.com/ilbumi/timemd/commit/9b773d784cb3d6bcf3f1f4c76d87b3cf8e81f2ac))
- **frontend:** redraw the PWA icons as the timer dial ([6e72ea2](https://github.com/ilbumi/timemd/commit/6e72ea2bcc169694900ff612bf5a451556214012))
- **frontend:** lay the app out for a desktop as well as a phone ([5233024](https://github.com/ilbumi/timemd/commit/523302459555d9d22d9842536aaf663e58570f06))

### Changed

- **cli:** split the command model out of the binary shim ([bfc1bd6](https://github.com/ilbumi/timemd/commit/bfc1bd6b8e251cf90c3e34b837e52006103c625d))
- **frontend:** centralise ISO date parsing ([2585fa3](https://github.com/ilbumi/timemd/commit/2585fa39d15be33c7a1d09379174339642c2137f))
- remove duplication surfaced by a cleanup review ([5b7eb05](https://github.com/ilbumi/timemd/commit/5b7eb05a90d6e50f8c2c2bb5c57b9f6ec560e626))
- fold duplication surfaced by a cleanup review ([87ec97e](https://github.com/ilbumi/timemd/commit/87ec97e95f69aba1620bac52c790a902fabd6b6c))
- **frontend:** decide the content split by the column, not the window ([6b57805](https://github.com/ilbumi/timemd/commit/6b5780548bf744656280d97ec772b2802715da22))
- **frontend:** delete the declarations that do nothing ([14c65fb](https://github.com/ilbumi/timemd/commit/14c65fb03b433ba3bb4aefb5be35ffc8a38637df))

### Fixed

- **frontend:** close the tablet gap and the sub-44px tap targets ([ff343e4](https://github.com/ilbumi/timemd/commit/ff343e443614d22bddbf38636bc7e562b3bd6546))
- **frontend:** stop the schedule blocks drawing doubled and lopsided ([6fc1b2f](https://github.com/ilbumi/timemd/commit/6fc1b2fd85d1fbb78bc7dc4e592e1b4a785e459d))
- **frontend:** drop the white ring the project tiles grew on hover ([3644f0e](https://github.com/ilbumi/timemd/commit/3644f0ee869196a90a9d2af278731038d6cacc7a))
- **frontend:** keep the schedule's marks inside the boxes they belong to ([3e1c36a](https://github.com/ilbumi/timemd/commit/3e1c36a9482047d4463eaac957fa96d7d05ae89c))
- **frontend:** end a screen's rules where its content ends ([2140a19](https://github.com/ilbumi/timemd/commit/2140a1952b9eff71954bd4138aa4becb72d662b7))
- **frontend:** align the timer's edges, and land the breakpoint split properly ([ecacf01](https://github.com/ilbumi/timemd/commit/ecacf0114a8e3038ff8a481a0a0bf45741cfa114))
- **frontend:** give a footer its padding wherever it sits ([8db9531](https://github.com/ilbumi/timemd/commit/8db9531ca33c94b3904f977d8034b956f273c995))
- **frontend:** give the two small controls a thumb's reach ([fb8d477](https://github.com/ilbumi/timemd/commit/fb8d4770b9582b85fc06a4b8ca4d511314ac1a17))
- **frontend:** land the back arrow on the same edge as the fields below it ([3d1c781](https://github.com/ilbumi/timemd/commit/3d1c781729cadbb0c928874d7c7b7dcdc64af922))
- **frontend:** keep the sidebar and content clear of a device's insets ([47edf74](https://github.com/ilbumi/timemd/commit/47edf74b3c67d47cdc72e6d7f1bffe6ddf7dc6b9))
- **frontend:** end the complete screen's rules where its content ends ([abab330](https://github.com/ilbumi/timemd/commit/abab3302a078ab1c3f26df914b60c9ccd6d19790))
- **frontend:** stop the project shelf ending on a black hole ([f855196](https://github.com/ilbumi/timemd/commit/f8551960556a8a0388770129e99b98cf544a1f9a))
- **frontend:** end the project row's rule where the row ends ([020fc7e](https://github.com/ilbumi/timemd/commit/020fc7e43cda4fde6a08d2f3bfef35c27f079ccb))
- **frontend:** sit a top bar's title and its right-hand label on one baseline ([d244af7](https://github.com/ilbumi/timemd/commit/d244af77bd2db7f1728573321a5c1c3a8441e34f))
- **frontend:** keep the picker's selection rule inside the picker ([651047d](https://github.com/ilbumi/timemd/commit/651047d5522272f90c9e08d9cde23b5b2614d499))
- **frontend:** draw the weekly-target slider in the design's own language ([c8caac7](https://github.com/ilbumi/timemd/commit/c8caac70917eafa277981f775c1027022034ae68))
- **frontend:** give the phone column an edge when the window is wider than it ([4210223](https://github.com/ilbumi/timemd/commit/42102239032f494628ff3d0fcc2e1f1d4aeb1fd8))
- **frontend:** size the timeline in the shell's own viewport unit ([a5e550c](https://github.com/ilbumi/timemd/commit/a5e550c9229d3bf3e2c2ee76c2d47ff2de07aff9))
- **frontend:** end the week of squares on the same edge as the fields ([551e618](https://github.com/ilbumi/timemd/commit/551e61815c1b9d2babc9033ab460c2f922fc3d16))
- **mcp:** wrap the two list results in an object ([bc2e7e5](https://github.com/ilbumi/timemd/commit/bc2e7e5a2c1a291a4edba335b3f43b26b9728196))
- **server:** compile without a frontend build ([f4f6592](https://github.com/ilbumi/timemd/commit/f4f6592840ee1d8d26893c8852c29d143db3d418))
- **core:** keep a hand-written heading's spacing through a write ([f6a1518](https://github.com/ilbumi/timemd/commit/f6a151855c6b5659487ed3fe0b45339e6ddb3d90))
- **ci:** write the changelog on a first release (#12) ([1e9c5a6](https://github.com/ilbumi/timemd/commit/1e9c5a67635403df98ac2580aee6987bd051064f))
