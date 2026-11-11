// System clipboard through Qt (#684).
//
// The Rust side used arboard, whose Wayland path needs the wlr/ext-data-control
// protocol. KWin has it; Mutter (GNOME) does not, so arboard fell back to X11
// there — which works natively through XWayland and FAILS inside the Flatpak
// sandbox (no X11 socket): "clipboard unavailable: X11 server connection timed
// out". QClipboard speaks the base wl_data_device protocol every compositor
// implements, needs no extra sandbox permission, and the offer stays alive for
// as long as the application does (the #514 lifetime rule, for free).
//
// The clipboard is a GUI-thread object; the call is queued onto the
// application's thread so any caller (a cxx-qt invokable already on it, or a
// tokio worker) is fine.

#include <QtCore/QCoreApplication>
#include <QtCore/QMetaObject>
#include <QtCore/QString>
#include <QtGui/QClipboard>
#include <QtGui/QGuiApplication>

extern "C" void qbz_clipboard_set_text(const char *utf8, int len) {
    QCoreApplication *core = QCoreApplication::instance();
    if (!core || !qobject_cast<QGuiApplication *>(core)) {
        return;
    }
    const QString text = QString::fromUtf8(utf8, len);
    QMetaObject::invokeMethod(
        core,
        [text]() {
            if (QClipboard *clipboard = QGuiApplication::clipboard()) {
                clipboard->setText(text, QClipboard::Clipboard);
            }
        },
        Qt::QueuedConnection);
}
