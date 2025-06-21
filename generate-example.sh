#!/usr/bin/env bash

x() {
	echo "\$ $*"
	"$@"
	echo
}

x wrun
x cd foo
x wrun --all
echo '# Run local tasks with just their name'
x wrun hola
echo '# or tasks from elsewhere in the project with fully qualified syntax'
x wrun /format
echo "# or even fully qualified local tasks, though currently this won't tab complete"
x wrun foo/dir
x cd ..
echo '# Tasks are not deduplicated in case they have side effects'
x wrun format foo/dir /format
