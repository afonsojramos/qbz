#include <QtCore/QCoreApplication>
#include <QtCore/QEvent>
#include <QtCore/QObject>
#include <QtGui/QKeyEvent>

namespace {
// Lifecycle commands are reserved even while a text field or mini window
// owns focus. Qt maps ControlModifier to Command on macOS.
class LifecycleFilter final : public QObject {
public:
    LifecycleFilter(QObject *parent, void (*request)(bool))
        : QObject(parent), request_(request) {}

protected:
    bool eventFilter(QObject *receiver, QEvent *event) override {
        if (receiver == QCoreApplication::instance() && event->type() == QEvent::Quit) {
            event->ignore(); // termination is pending until QML confirms
            request_(true);
            return true;
        }
        if (event->type() != QEvent::KeyPress && event->type() != QEvent::ShortcutOverride)
            return false;
        auto *key = static_cast<QKeyEvent *>(event);
        if (key->modifiers() != Qt::ControlModifier
            || (key->key() != Qt::Key_Q && key->key() != Qt::Key_W))
            return false;
        event->accept();
        if (event->type() == QEvent::KeyPress && !key->isAutoRepeat())
            request_(key->key() == Qt::Key_Q);
        return true;
    }

private:
    void (*request_)(bool);
};
}

extern "C" void qbz_install_lifecycle_filter(void (*request)(bool)) {
    static LifecycleFilter *filter = nullptr;
    auto *app = QCoreApplication::instance();
    if (!filter && app) {
        filter = new LifecycleFilter(app, request);
        app->installEventFilter(filter);
    }
}
