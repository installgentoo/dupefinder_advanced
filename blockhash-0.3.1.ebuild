# Copyright 1999-2019 Gentoo Authors
# Distributed under the terms of the GNU General Public License v2

EAPI=7

DESCRIPTION="Blockhash image hash."
HOMEPAGE="https://github.com/commonsmachinery/blockhash"
SRC_URI="https://github.com/commonsmachinery/blockhash/archive/v${PV}.tar.gz"

LICENSE="GPL-2+"
SLOT="0"
KEYWORDS="~amd64 ~arm ~x86"
IUSE=""

DEPEND="media-gfx/imagemagick"

src_compile() {
	gcc -o blockhash ./blockhash.c -O3 -D MAGICKWAND_V7 -I /usr/include/ImageMagick-7 -lm -l /usr/lib64/libMagickWand-7.Q16.so
}

src_install() {
	insinto /usr/bin/
	insopts -m0755
	doins blockhash
}
