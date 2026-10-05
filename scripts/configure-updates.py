#!/usr/bin/env python3
"""Populate Sparkle distribution settings; never embed a private key."""
import base64, os, plistlib, sys, urllib.parse
path=sys.argv[1]
with open(path,'rb') as file: info=plistlib.load(file)
feed=os.environ.get('TASKBAR_UPDATE_FEED_URL','')
key=os.environ.get('TASKBAR_UPDATE_PUBLIC_KEY','')
if bool(feed)!=bool(key): raise SystemExit('Set both TASKBAR_UPDATE_FEED_URL and TASKBAR_UPDATE_PUBLIC_KEY')
if feed:
    url=urllib.parse.urlsplit(feed)
    if url.scheme!='https' or not url.hostname or url.username or url.password: raise SystemExit('Release appcasts require HTTPS without URL credentials')
    try: valid=len(base64.b64decode(key,validate=True))==32
    except ValueError: valid=False
    if not valid: raise SystemExit('Sparkle public key must encode 32 bytes')
    info.update(SUFeedURL=feed,SUPublicEDKey=key)
with open(path,'wb') as file: plistlib.dump(info,file)
