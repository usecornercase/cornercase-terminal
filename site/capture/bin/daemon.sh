#!/bin/sh
exec 3>> "$1"
while [ -d "$2" ]; do sleep 0.5; done
