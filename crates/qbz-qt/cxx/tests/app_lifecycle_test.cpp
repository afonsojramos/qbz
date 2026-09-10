#include <QtCore/QCoreApplication>
#include <QtCore/QEvent>
#include <QtGui/QKeyEvent>
#include <cassert>

extern "C" void qbz_install_lifecycle_filter(void (*request)(bool));
static int quits = 0;
static int closes = 0;
static void request(bool quit) { quit ? ++quits : ++closes; }

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    qbz_install_lifecycle_filter(request);
    qbz_install_lifecycle_filter(request); // no duplicate installation
    QObject focused;
    QKeyEvent override(QEvent::ShortcutOverride, Qt::Key_W, Qt::ControlModifier);
    QCoreApplication::sendEvent(&focused, &override);
    assert(override.isAccepted() && closes == 0);
    QKeyEvent close(QEvent::KeyPress, Qt::Key_W, Qt::ControlModifier);
    QCoreApplication::sendEvent(&focused, &close);
    assert(closes == 1 && quits == 0);
    QKeyEvent repeat(QEvent::KeyPress, Qt::Key_W, Qt::ControlModifier, {}, true);
    QCoreApplication::sendEvent(&focused, &repeat);
    assert(closes == 1);
    QKeyEvent unrelated(QEvent::KeyPress, Qt::Key_Q, Qt::ControlModifier | Qt::ShiftModifier);
    QCoreApplication::sendEvent(&focused, &unrelated);
    assert(quits == 0);
    QKeyEvent quit(QEvent::KeyPress, Qt::Key_Q, Qt::ControlModifier);
    QCoreApplication::sendEvent(&focused, &quit);
    assert(quits == 1);
    QEvent nativeQuit(QEvent::Quit);
    QCoreApplication::sendEvent(&app, &nativeQuit);
    assert(!nativeQuit.isAccepted() && quits == 2 && closes == 1);
}
