# Reference: Original User Prompt & Information

> Reproduced verbatim from the user's opening message on 2026-07-13. This is the
> source-of-truth record of the original request and environment; preserved as-is
> for reference.

---

my setup
- windows 
-- obsidian desktop
-- git plugin in obsidian - https://github.com/vinzent03/obsidian-git 
-- github private repo in remote
-- github repo closed on filesystem using personal access token in url 
-- git plugin in obsidian configured to sync every 2 mins & on open & close app
-- git properly configured on my machine

- android 
-- obsidian app fro mplay store 
-- git plugin - https://github.com/vinzent03/obsidian-git
-- gitsync androidn app - https://github.com/ViscousPot/GitSync
-- gitsync app configured with my provate fgithub repo , via github login t, access granted to all repos in my provate account


- github repo - private - https://github.com/tars003/obisidian-git-sync

- GOAL - i own a kindle paperwhite 7th gen, jailbreaked & koreader installed
- currently ko reader has a setup , where highlights of my files can be synced to my obsidian vaul
- my requirtement is on the opposite end 
- i want to be able to read my markdown files on my kindle , which i ingest using androind & windows
- currently, syncing between android & windows works flawlessly
- koreader supports md file rendering - so not much trouble
- koreader also supports syncthing, im not sure if i can configure syncthing to pull from github repo, but assyuming i can, our life is easy, hwoever incase we are not, we might have to install syncthing on windows to push data to github repo & syncthing both, androuidn can keep pulliung from github repo m & kindle can pullf rom syncthing
- however, the challenge
-- these md files contain [[]] links to other md files in my obsidian vault
-- these md files also contain images in http format, 
-- https://github.com/obsidianmd/obsidian-clipper - i use this plugin in firefox to save articles as md files in my vault, so images get encoded as url, obsdian on mnile & deskltop renders these images , kindle ko reader cannot by defaiult
- so we are left with these 2 main challenges
- i do not need highlughtling for now
- so essentially we have to sync the obsidian to kindle , and tackle these 2 challenges properlu
- default md viewer works finr for reading excep tthese 2 challenges

KOREADER APP DEVELOPEMENT - https://koreader.rocks/doc/topics/Development_guide.md.html

i want to develop a koreader plugin to make this possible

what do you think ?
use websearch plugin wherever needed
