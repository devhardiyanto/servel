#!/bin/sh
# kova: Kova CLI for macOS and Linux.
#
# Service and project commands are carried out by the running Kova app over
# its local control channel, so the app and the terminal always agree.
# status, logs and shell talk to Docker directly and work without the app.
#
# Installed by Kova (Settings > General > Install the kova command).

set -u

APP_ID='com.devhardiyanto.servel'
PREFIX='servel_'

say()  { printf '[kova] %s\n' "$*"; }
fail() { printf '[kova] %s\n' "$*" >&2; exit 1; }

data_dir() {
  case "$(uname -s)" in
    Darwin) printf '%s\n' "$HOME/Library/Application Support/$APP_ID" ;;
    *)      printf '%s\n' "${XDG_DATA_HOME:-$HOME/.local/share}/$APP_ID" ;;
  esac
}

need_docker() {
  command -v docker >/dev/null 2>&1 || fail 'docker was not found. Install Docker (Desktop or Engine) first.'
}

# Send one request to the running app. Prints the reply; returns 1 when the
# app is not running.
control_raw() {
  file="$(data_dir)/control.json"
  [ -r "$file" ] || return 1
  port=$(sed -n 's/.*"port"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' "$file")
  token=$(sed -n 's/.*"token"[[:space:]]*:[[:space:]]*"\([0-9a-f]*\)".*/\1/p' "$file")
  [ -n "$port" ] && [ -n "$token" ] || return 1
  if command -v bash >/dev/null 2>&1; then
    reply=$(KOVA_REQ="$token plain $1" KOVA_PORT="$port" bash -c '
      exec 3<>"/dev/tcp/127.0.0.1/$KOVA_PORT" || exit 1
      printf "%s\n" "$KOVA_REQ" >&3
      cat <&3' 2>/dev/null)
  elif command -v nc >/dev/null 2>&1; then
    # nc keeps reading after stdin ends, until the app closes the connection.
    reply=$(printf '%s plain %s\n' "$token" "$1" | nc -w 120 127.0.0.1 "$port" 2>/dev/null)
  else
    fail 'neither bash nor nc is available to reach the Kova app.'
  fi
  [ -n "$reply" ] || return 1
  printf '%s\n' "$reply"
}

# Run a request and print the result as a message plus a table.
control() {
  reply=$(control_raw "$1") || fail 'the Kova app is not running. Open Kova first.'
  status=$(printf '%s\n' "$reply" | sed -n 1p)
  message=$(printf '%s\n' "$reply" | sed -n 2p)
  if [ "$status" != ok ]; then
    fail "${message:-request failed}"
  fi
  [ -n "$message" ] && say "$message"
  printf '%s\n' "$reply" | awk -F '\t' '
    NR <= 2 { next }
    $1 == "project" { if (!p++) printf "%-28s %-14s %s\n", "PROJECT", "STATE", "URL"; printf "%-28s %-14s %s\n", $2, $3, $4 }
    $1 == "service" { if (!s++) printf "%-14s %s\n", "SERVICE", "STATE"; printf "%-14s %s\n", $2, $3 }
    $1 == "catalog" { if (!c++) printf "%-14s %-34s %s\n", "SERVICE", "IMAGE", "PORTS"; printf "%-14s %-34s %s\n", $2, $3, $4 }
    END { if (!p && !s && !c && want) print "[kova] " want }
  ' want="${2:-}"
}

container() {
  [ -n "${1:-}" ] || fail "give the service name, for example: kova $2 mysql"
  printf '%s%s\n' "$PREFIX" "$1"
}

cmd_status() {
  # With the app: its own view (includes services that were never created).
  if reply=$(control_raw services) && [ "$(printf '%s\n' "$reply" | sed -n 1p)" = ok ]; then
    control services 'no services yet.'
    return
  fi
  need_docker
  docker ps -a --filter "name=^${PREFIX}" --format 'table {{.Names}}\t{{.State}}\t{{.Status}}\t{{.Ports}}'
}

cmd_logs() {
  need_docker
  name=$(container "${1:-}" logs) || exit 1
  shift
  follow=''; lines=100
  while [ $# -gt 0 ]; do
    case "$1" in
      -f|--follow) follow='-f' ;;
      -n) shift; lines="${1:-100}" ;;
      *) fail "unknown option for logs: $1" ;;
    esac
    shift
  done
  # shellcheck disable=SC2086
  exec docker logs --tail "$lines" $follow "$name"
}

cmd_shell() {
  need_docker
  name=$(container "${1:-}" shell) || exit 1
  shift
  if [ $# -gt 0 ]; then exec docker exec -it "$name" "$@"; fi
  exec docker exec -it "$name" sh
}

# Restore runs inside the container, reading the dump from stdin, so nothing
# is copied and the file can be any size.
cmd_restore() {
  [ $# -ge 2 ] || fail 'usage: kova restore <mysql|postgres> <file.sql | folder of .sql files>'
  id="$1"; target="$2"
  case "$id" in
    mysql)
      ready='MYSQL_PWD="${MYSQL_ROOT_PASSWORD:-}" mysqladmin ping -h 127.0.0.1 -uroot --silent'
      load='MYSQL_PWD="${MYSQL_ROOT_PASSWORD:-}" mysql -uroot' ;;
    postgres)
      ready='pg_isready -h 127.0.0.1 -U "${POSTGRES_USER:-postgres}" -q'
      load='psql -v ON_ERROR_STOP=1 -q -U "${POSTGRES_USER:-postgres}" -d postgres' ;;
    *) fail 'restore works only for: mysql, postgres' ;;
  esac
  [ -e "$target" ] || fail "not found: $target"
  if [ -d "$target" ]; then
    ls "$target"/*.sql >/dev/null 2>&1 || fail "no .sql files in $target"
  fi
  need_docker
  name="${PREFIX}${id}"

  control "up $id"
  say "waiting for $name to be ready (up to 120 s)..."
  i=0
  until docker exec "$name" sh -c "$ready" >/dev/null 2>&1; do
    i=$((i + 1))
    [ "$i" -lt 60 ] || fail "$name was not ready within 120 seconds. Check: kova logs $id"
    sleep 2
  done

  restore_one() {
    say "restore $(basename "$1") -> $name"
    docker exec -i "$name" sh -c "$load" < "$1" || fail "restore $(basename "$1") failed."
    say "finished $(basename "$1")"
  }
  if [ -d "$target" ]; then
    # The glob expands in name order, the same order the Windows CLI uses.
    for f in "$target"/*.sql; do restore_one "$f"; done
  else
    restore_one "$target"
  fi
}

cmd_use() {
  command -v mise >/dev/null 2>&1 || fail 'mise was not found. Install it first: https://mise.jdx.dev'
  global=''; spec=''
  for a in "$@"; do
    case "$a" in
      -g|--global) global='-g' ;;
      *@*) spec="$a" ;;
      *) if [ -z "$spec" ]; then spec="$a"; else spec="$spec@$a"; fi ;;
    esac
  done
  case "$spec" in
    *@*) ;;
    *) fail 'usage: kova use <tool>@<version> [--global], for example: kova use php@8.4' ;;
  esac
  # shellcheck disable=SC2086
  exec mise use $global "$spec"
}

cmd_preflight() {
  need_docker
  say "docker: $(docker --version 2>/dev/null)"
  if docker info >/dev/null 2>&1; then say 'docker engine: running'; else say 'docker engine: not responding (start Docker)'; fi
  if command -v mise >/dev/null 2>&1; then say "mise: $(mise --version 2>/dev/null | sed -n 1p)"; else say 'mise: not installed (versions cannot be switched)'; fi
  if control_raw status >/dev/null; then say 'Kova app: running'; else say 'Kova app: not running (up, down and project commands need it)'; fi
}

show_help() {
  cat <<'HELP'
kova: manage Kova services and projects from the terminal

  kova up [service...]           start services (default: the selection in the app)
  kova down [service...]         stop services (default: all of them)
  kova status                    status of every service
  kova list                      list the available services
  kova logs <service> [-f] [-n N]
  kova shell <service> [cmd...]  shell or command inside the container
  kova restore <mysql|postgres> <file.sql | folder>
                                 load a SQL dump (a folder is loaded in name order)
  kova use <tool>@<version> [--global]
                                 change a version through mise in this folder (or globally)
  kova start <project>           start the project's dev server
  kova stop <project>            stop the project's dev server
  kova restart <project>         stop, then start again
  kova ps                        dev server status of every project
  kova preflight                 check Docker, mise and the Kova app
  kova service <up|down|status|list|logs|shell> ...
                                 same as the service commands above

up, down, list, restore, start, stop, restart and ps need the Kova app to be open.
HELP
}

main() {
  cmd="${1:-help}"
  [ $# -gt 0 ] && shift
  case "$cmd" in
    help|-h|--help) show_help ;;
    up)        control "up $*" ;;
    down)      control "down $*" ;;
    status)    cmd_status ;;
    list)      control list 'no services are defined.' ;;
    restore)   cmd_restore "$@" ;;
    logs)      cmd_logs "$@" ;;
    shell)     cmd_shell "$@" ;;
    use)       cmd_use "$@" ;;
    preflight) cmd_preflight ;;
    start|stop|restart)
      [ $# -gt 0 ] || fail "give the project name: kova $cmd <project>"
      control "$cmd $*" ;;
    ps|projects) control status 'no project has a dev server command yet.' ;;
    service)
      sub="${1:-}"
      [ $# -gt 0 ] && shift
      case "$sub" in
        up|down|status|list|logs|shell) main "$sub" "$@" ;;
        *) fail 'usage: kova service <up|down|status|list|logs|shell> [args...]' ;;
      esac ;;
    *) printf '[kova] unknown command: %s\n\n' "$cmd" >&2; show_help >&2; exit 2 ;;
  esac
}

main "$@"
