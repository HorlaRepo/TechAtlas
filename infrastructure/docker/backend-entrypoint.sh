#!/bin/sh
set -eu

case "${1:-}" in
    api)
        shift
        exec /usr/local/bin/techatlas-api "$@"
        ;;
    cli)
        shift
        exec /usr/local/bin/techatlas-cli "$@"
        ;;
    scheduler)
        shift
        exec /usr/local/bin/techatlas-scheduler "$@"
        ;;
    worker)
        shift
        exec /usr/local/bin/techatlas-worker "$@"
        ;;
    *)
        echo "usage: techatlas-entrypoint {api|cli|scheduler|worker} [arguments...]" >&2
        exit 64
        ;;
esac
