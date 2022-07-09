#!/bin/bash
set -e
POSITIONAL_ARGS=()
MODE="dev"
INSTALL_PATH="$HOME/.cargo/bin/pgwm"
while [[ $# -gt 0 ]]; do
  case $1 in
    -d)
      MODE="dev"
      shift # past argument
      ;;
    -l)
      MODE="lto"
      shift # past argument
      ;;
    -p)
      INSTALL_PATH="$2"
      shift # past argument
      shift # past value
      ;;
    -*|--*)
      echo "Unknown option $1"
      exit 1
      ;;
    *)
      POSITIONAL_ARGS+=("$1") # save positional arg
      shift # past argument
      ;;
  esac
done

echo "MODE                     = ${MODE}"
echo "INSTALL_PATH             = ${INSTALL_PATH}"

if [[ "dev" == $MODE ]]; then
  cargo b -p pgwm --profile $MODE --features debug && install target/debug/pgwm "$INSTALL_PATH"
  exit 0
fi

if [[ "lto" == $MODE ]]; then
  /usr/bin/cargo b -p pgwm --profile "$MODE" --target x86_64-unknown-linux-musl && install target/x86_64-unknown-linux-musl/lto/pgwm "$INSTALL_PATH"
  exit 0
fi
