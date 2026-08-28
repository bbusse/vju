# Maintainer: Björn Busse <bj.rn@baerlin.eu>
pkgname=vju
# Placeholder: releases pass their computed version to build-apk.yml, which
# rewrites this line before abuild runs. Only branch builds ship it as-is
pkgver=0.1.0
pkgrel=0
pkgdesc="A window with a vju"
url="https://github.com/bbusse/vju"
arch="all"
license="MIT OR Apache-2.0"
depends="libx11 libxkbcommon wayland-libs-client wayland-libs-egl alsa-lib eudev-libs"
makedepends="cargo rust pkgconf libx11-dev libxkbcommon-dev wayland-dev mesa-dev alsa-lib-dev eudev-dev"
# The glow-rendered build ships alongside the default wgpu one rather than
# replacing it: wgpu needs compute shaders, which a software-only GL stack
# does not provide
subpackages="$pkgname-glow"
# Built directly from this checkout (no source= fetch), so builddir points
# straight at $startdir. srcdir is redirected off to the side: abuild's
# default srcdir ($startdir/src) collides with - and gets wiped by abuild
# before build() runs - this project's own src/ directory
srcdir="$startdir/.abuild-src"
builddir="$startdir"

build() {
	cd "$builddir"
	cargo build --release --locked
	cargo build --release --locked --no-default-features \
		--features glow-renderer --target-dir glowtarget
}

package() {
	cd "$builddir"
	install -Dm755 target/release/vju "$pkgdir"/usr/bin/vju
}

glow() {
	pkgdesc="$pkgdesc, rendered through glow instead of wgpu"
	depends="libxkbcommon wayland-libs-client wayland-libs-egl alsa-lib eudev-libs"
	install -Dm755 "$builddir"/glowtarget/release/vju \
		"$subpkgdir"/usr/bin/vju-glow
}
