#!/bin/bash
set -e
ensure_no_previous_make() {
  if [ -f Makefile ]; then
    rm Makefile
  fi
}
check_clean() {
  if [ -d target ]; then
    echo "Target still exists after clean"
    exit 1
  fi
}

check_build() {
  if [ ! -f "target/$1/pgwm" ]; then
    echo "Build did not produce a binary at expected location target/$1/pgwm"
    exit 1
  fi
}

check_install() {
  if [ -z "$1" ]; then
    DIR=$HOME/.local/bin/pgwm
  else
    DIR=$1/pgwm
  fi
  if [ -z "$2" ]; then
    CFG=$HOME/.config/pgwm/pgwm.toml
  else
    CFG=$2/pgwm.toml
  fi
  if [ ! -f "$DIR" ]; then
    echo "Binary not placed in expected location $DIR"
    exit 1
  fi
  if [ ! -f "$CFG" ]; then
    echo "Configuration not placed in expected location $CFG"
    exit 1
  fi
}

check_uninstall() {
  if [ -z "$1" ]; then
    DIR=$HOME/.local/bin/pgwm
  else
    DIR=$1/pgwm
  fi
  if [ -z "$2" ]; then
    CFG=$HOME/.config/pgwm/pgwm.toml
  else
    CFG=$2/pgwm.toml
  fi
  if [ -f "$DIR" ]; then
    echo "Binary still present after uninstalling"
    exit 1
  fi

  if [ "$3" -eq 1 ]; then
    if [ -f "$CFG" ]; then
      echo "Configuration still present after uninstalling at $CFG"
      exit 1
    fi
  fi
}

ensure_no_previous_make
./configure --profile=release
make clean
check_clean
make
check_build release
make install
check_install "$DEAD_VAR1" "$DEAD_VAR2"
make uninstall
check_uninstall "$DEAD_VAR1" "$DEAD_VAR2" 0
make uninstall CLEAN_CONFIG=1
check_uninstall "$DEAD_VAR1" "$DEAD_VAR2" 1

ensure_no_previous_make
./configure --profile=release --bin-dir=docs --config-dir=docs
make clean
check_clean
make
check_build release
make install
check_install docs docs
make uninstall
check_uninstall docs docs 0
make uninstall CLEAN_CONFIG=1
check_uninstall docs docs 1

ensure_no_previous_make
./configure --profile=lto
OPTIMIZED_MAKEFILE=$(cat Makefile)
ensure_no_previous_make
./configure
NO_ARG_MAKEFILE=$(cat Makefile)
if [ "$OPTIMIZED_MAKEFILE" != "$NO_ARG_MAKEFILE" ]; then
  echo "Default profile isn't 'lto'"
  exit 1
fi
make clean
check_clean
make
check_build lto
make install
check_install "$DEAD_VAR1" "$DEAD_VAR2"
make uninstall
check_uninstall "$DEAD_VAR1" "$DEAD_VAR2" 0
make uninstall CLEAN_CONFIG=1
check_uninstall "$DEAD_VAR1" "$DEAD_VAR2" 1

