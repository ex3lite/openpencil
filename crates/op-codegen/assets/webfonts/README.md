Standalone HTML embeds the faces used in the exported selection. These WOFF2
files encode the original editor fonts in op-host-desktop/assets/fonts and
op-host-native/assets/Roboto-Regular.ttf. They retain the glyph set, OpenType
features, variable axes and embedded name and license records.

From the repository root, regenerate one face with fontTools and its Brotli
backend, for example:

    fonttools ttLib.woff2 compress crates/op-host-desktop/assets/fonts/Inter-VF.ttf -o crates/op-codegen/assets/webfonts/Inter-VF.woff2

The browser-hosted generator reuses the TTF files already loaded by the editor.
