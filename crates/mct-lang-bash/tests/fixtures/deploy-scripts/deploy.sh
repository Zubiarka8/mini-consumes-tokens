source ./lib.sh

VERSION=1.0.0

build() {
  log "building $VERSION"
}

deploy() {
  build
  log "deployed"
}
