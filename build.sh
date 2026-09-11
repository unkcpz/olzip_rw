#!/usr/bin/env bash

SRC=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

if [ "$1" = "clean" ];then 
    rm -f ${SRC}/seek_pat.txt
    echo "clean test files.\n"
fi
