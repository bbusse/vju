# Maintainer: Björn Busse <bj.rn@baerlin.eu>
pkgname=vju
pkgver=0.1.0
pkgrel=0
pkgdesc="A window with a vju"
url="https://github.com/bbusse/vju"
arch="all"
license="MIT OR Apache-2.0"
depends="libx11 libxkbcommon wayland-libs-client mesa-gl alsa-lib eudev-libs"
makedepends="cargo rust pkgconf libx11-dev libxkbcommon-dev wayland-dev mesa-dev alsa-lib-dev eudev-dev"
# Built directly from this checkout (no source= fetch), so builddir points
# straight at $startdir. srcdir is redirected off to the side: abuild's
# default srcdir ($startdir/src) collides with - and gets wiped by abuild
# before build() runs - this project's own src/ directory
srcdir="$startdir/.abuild-src"
builddir="$startdir"

build() {
	cd "$builddir"
	cargo build --release --locked
}

package() {
	cd "$builddir"
	install -Dm755 target/release/vju "$pkgdir"/usr/bin/vju
}
