// Windows shell helpers with no Q_OBJECT of their own.
//
// Compiled on EVERY platform on purpose: the bodies are portable Qt and only
// the caller's use of the WId is Windows-specific. Keeping it unconditional
// means the file is type-checked by the Linux and macOS builds too, which is
// where a signature drift would otherwise go unnoticed until a Windows CI run.

#include <QtCore/QObject>
#include <QtGui/QGuiApplication>
#include <QtGui/QSessionManager>
#include <QtGui/QWindow>

// The top-level window's native handle, or nullptr before one is shown.
//
// MUST NOT be called before exec(): QWindow::winId() CREATES the platform
// window as a side effect, so an early call would hand back a handle for a
// window Qt has not finished setting up, and the real one would then get a
// different HWND. Callers hop to the GUI thread first; winId() is not
// thread-safe.
extern "C" void *qbz_main_window_hwnd()
{
    const auto windows = QGuiApplication::topLevelWindows();
    for (QWindow *w : windows)
        if (w->isVisible() && w->type() == Qt::Window)
            return reinterpret_cast<void *>(w->winId());
    return nullptr;
}

// WM_QUERYENDSESSION -> QGuiApplication::commitDataRequest, via Qt's Windows
// session manager. It fires BEFORE Windows decides whether to proceed with the
// logoff and Qt blocks until the handler returns, which makes it the one safe
// place to persist synchronously: anything deferred to a queued connection or
// another thread races the process being killed.
extern "C" void qbz_install_commit_data_handler(void (*cb)())
{
#ifndef QT_NO_SESSIONMANAGER
    // Once. Qt::UniqueConnection does NOT deduplicate lambdas -- each connect
    // makes an independent connection -- so a second install would run the
    // session persist twice on every logoff.
    static bool installed = false;
    if (installed)
        return;
    installed = true;

    QObject::connect(qApp, &QGuiApplication::commitDataRequest, qApp,
                     [cb](QSessionManager &) { cb(); });
#else
    // A Qt built without the session manager has no commitDataRequest at all,
    // and this file compiles on every platform.
    (void)cb;
#endif
}
