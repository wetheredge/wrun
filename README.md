# wrun

`wrun` is a simple task runner built for monorepos. (Specifically [this one][VerTX].)

Example from this repo:

```shell
$ wrun
Local:
  hallo               toki!
  one-task            to rule them all and in the darkness run them
  format              Something actually useful

$ cd foo

$ wrun --all
Local:
  hola                bonjour
  dir                 Tasks always run in the directory they are defined in
In /:
  hallo               toki!
  one-task            to rule them all and in the darkness run them
  format              Something actually useful

# Run local tasks with just their name
$ wrun hola
wrun(foo/hola): echo hej fra foo!
hej fra foo!

# or tasks from elsewhere in the project with fully qualified syntax
$ wrun /format
wrun(/indirectly-rustfmt): cargo +nightly fmt

# or even fully qualified local tasks, though currently this won't tab complete
$ wrun foo/dir
wrun(foo/dir): basename $(pwd)
foo

$ cd ..

# Tasks are not deduplicated in case they have side effects
$ wrun format foo/dir /format
wrun(/indirectly-rustfmt): cargo +nightly fmt
wrun(foo/dir): basename $(pwd)
foo
wrun(/indirectly-rustfmt): cargo +nightly fmt
```

## Completions

```shell
# Bash
$ echo "source <(COMPLETE=bash wrun)" >> ~/.bashrc

# Elvish
$ echo "eval (E:COMPLETE=elvish wrun | slurp)" >> ~/.elvish/rc.elv

# Fish
$ echo "source (COMPLETE=fish wrun | psub)" >> ~/.config/fish/config.fish

# Zsh
$ echo "source <(COMPLETE=zsh wrun)" >> ~/.zshrc
```

[VerTX]: https://github.com/wetheredge/VerTX
