#include "scope_trace_item.h"

#include <QtCore/QCoreApplication>
#include <QtQml/qqml.h>
#include <QtQuick/QSGFlatColorMaterial>
#include <QtQuick/QSGGeometryNode>

ScopeTraceItem::ScopeTraceItem(QQuickItem *parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents, true);
    setAntialiasing(true);
}

void ScopeTraceItem::setPoints(const QVariantList &points)
{
    m_points = points;
    update();
}

void ScopeTraceItem::setTraceColor(const QColor &color)
{
    if (m_traceColor == color)
        return;
    m_traceColor = color;
    update();
}

void ScopeTraceItem::setMode(int mode)
{
    mode = mode == 1 ? 1 : 0;
    if (m_mode == mode)
        return;
    m_mode = mode;
    update();
}

void ScopeTraceItem::setLineWidth(qreal width)
{
    width = qMax<qreal>(1.0, width);
    if (qFuzzyCompare(m_lineWidth, width))
        return;
    m_lineWidth = width;
    update();
}

QSGNode *ScopeTraceItem::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *)
{
    const int count = m_mode == 0 ? m_points.size() / 2 : m_points.size();
    if (count < 2 || width() <= 0.0 || height() <= 0.0) {
        delete oldNode;
        return nullptr;
    }

    auto *node = static_cast<QSGGeometryNode *>(oldNode);
    if (!node) {
        node = new QSGGeometryNode;
        auto *geometry = new QSGGeometry(QSGGeometry::defaultAttributes_Point2D(), count);
        geometry->setDrawingMode(QSGGeometry::DrawLineStrip);
        geometry->setLineWidth(static_cast<float>(m_lineWidth));
        node->setGeometry(geometry);
        node->setFlag(QSGNode::OwnsGeometry);
        auto *material = new QSGFlatColorMaterial;
        material->setColor(m_traceColor);
        node->setMaterial(material);
        node->setFlag(QSGNode::OwnsMaterial);
    }

    auto *geometry = node->geometry();
    if (geometry->vertexCount() != count)
        geometry->allocate(count);
    geometry->setDrawingMode(QSGGeometry::DrawLineStrip);
    geometry->setLineWidth(static_cast<float>(m_lineWidth));
    auto *vertices = geometry->vertexDataAsPoint2D();
    const float pad = 4.0f;
    const float drawWidth = qMax(1.0f, static_cast<float>(width()) - pad * 2.0f);
    const float drawHeight = qMax(1.0f, static_cast<float>(height()) - pad * 2.0f);

    for (int index = 0; index < count; ++index) {
        float x;
        float y;
        if (m_mode == 0) {
            const float side = qBound(-1.0f, m_points[index * 2].toFloat(), 1.0f);
            const float mid = qBound(-1.0f, m_points[index * 2 + 1].toFloat(), 1.0f);
            x = pad + (0.5f + side * 0.46f) * drawWidth;
            y = pad + (0.5f - mid * 0.46f) * drawHeight;
        } else {
            const float value = qBound(-1.0f, m_points[index].toFloat(), 1.0f);
            x = pad + index * drawWidth / qMax(1, count - 1);
            y = pad + (0.5f - value * 0.46f) * drawHeight;
        }
        vertices[index].set(x, y);
    }

    static_cast<QSGFlatColorMaterial *>(node->material())->setColor(m_traceColor);
    node->markDirty(QSGNode::DirtyGeometry | QSGNode::DirtyMaterial);
    return node;
}

static bool g_scopeTraceRegistered = false;

extern "C" void qbz_scope_trace_register_qml_type()
{
    if (g_scopeTraceRegistered)
        return;
    g_scopeTraceRegistered = true;
    qmlRegisterType<ScopeTraceItem>("com.blitzfc.qbz", 1, 0, "ScopeTraceItem");
}

Q_COREAPP_STARTUP_FUNCTION(qbz_scope_trace_register_qml_type)
