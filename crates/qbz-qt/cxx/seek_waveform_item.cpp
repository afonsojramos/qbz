#include "seek_waveform_item.h"

#include <algorithm>
#include <atomic>
#include <cmath>
#include <limits>
#include <vector>

#include <QtQml/qqml.h>
#include <QtQuick/QSGClipNode>
#include <QtQuick/QSGFlatColorMaterial>
#include <QtQuick/QSGGeometryNode>

namespace {

QSGGeometryNode *makeLayer(const QColor &color)
{
    auto *node = new QSGGeometryNode;
    auto *geometry = new QSGGeometry(QSGGeometry::defaultAttributes_Point2D(), 0);
    geometry->setDrawingMode(QSGGeometry::DrawTriangleStrip);
    node->setGeometry(geometry);
    node->setFlag(QSGNode::OwnsGeometry);

    auto *material = new QSGFlatColorMaterial;
    material->setColor(color);
    node->setMaterial(material);
    node->setFlag(QSGNode::OwnsMaterial);
    return node;
}

class WaveformRootNode : public QSGNode
{
public:
    WaveformRootNode()
    {
        baseClip.setIsRectangular(true);
        cacheClip.setIsRectangular(true);
        playedClip.setIsRectangular(true);
        baseLayer = makeLayer(Qt::transparent);
        cacheLayer = makeLayer(Qt::transparent);
        playedLayer = makeLayer(Qt::transparent);
        baseClip.appendChildNode(baseLayer);
        cacheClip.appendChildNode(cacheLayer);
        playedClip.appendChildNode(playedLayer);
        appendChildNode(&baseClip);
        appendChildNode(&cacheClip);
        appendChildNode(&playedClip);
    }

    ~WaveformRootNode() override
    {
        removeChildNode(&baseClip);
        removeChildNode(&cacheClip);
        removeChildNode(&playedClip);
    }

    QSGClipNode baseClip;
    QSGClipNode cacheClip;
    QSGClipNode playedClip;
    QSGGeometryNode *baseLayer = nullptr;
    QSGGeometryNode *cacheLayer = nullptr;
    QSGGeometryNode *playedLayer = nullptr;
    quint64 geometryRevision = std::numeric_limits<quint64>::max();
    QSizeF geometrySize;
};

void setLayerColor(QSGGeometryNode *node, const QColor &color)
{
    auto *material = static_cast<QSGFlatColorMaterial *>(node->material());
    if (material->color() == color)
        return;
    material->setColor(color);
    node->markDirty(QSGNode::DirtyMaterial);
}

void setLayerGeometry(QSGGeometryNode *node,
                      const std::vector<float> &values,
                      qreal width,
                      qreal height)
{
    auto *geometry = node->geometry();
    const int count = static_cast<int>(values.size());
    geometry->allocate(count * 2);
    geometry->setDrawingMode(QSGGeometry::DrawTriangleStrip);
    auto *vertices = geometry->vertexDataAsPoint2D();
    const float centre = static_cast<float>(height * 0.5);
    const float halfHeight = std::max(1.0f, static_cast<float>(height * 0.48));

    for (int index = 0; index < count; ++index) {
        const float x = count == 1
            ? 0.0f
            : static_cast<float>(width) * index / static_cast<float>(count - 1);
        const float amplitude = std::clamp(values[index], 0.0f, 1.0f) * halfHeight;
        vertices[index * 2].set(x, centre - amplitude);
        vertices[index * 2 + 1].set(x, centre + amplitude);
    }
    node->markDirty(QSGNode::DirtyGeometry);
}

std::vector<float> renderValues(const QVariantList &source, int columns)
{
    if (source.isEmpty() || columns < 2)
        return {};

    std::vector<float> values(columns, 0.0f);
    const int sourceCount = source.size();
    for (int column = 0; column < columns; ++column) {
        const int begin = column * sourceCount / columns;
        const int end = std::max(begin + 1, (column + 1) * sourceCount / columns);
        float peak = 0.0f;
        for (int index = begin; index < std::min(end, sourceCount); ++index)
            peak = std::max(peak, std::clamp(source[index].toFloat(), 0.0f, 1.0f));
        values[column] = peak * peak;
    }

    std::vector<float> scratch(columns, 0.0f);
    for (int pass = 0; pass < 2; ++pass) {
        scratch.front() = values.front();
        scratch.back() = values.back();
        for (int index = 1; index + 1 < columns; ++index) {
            scratch[index] = values[index - 1] * 0.25f
                + values[index] * 0.5f
                + values[index + 1] * 0.25f;
        }
        values.swap(scratch);
    }
    return values;
}

} // namespace

SeekWaveformItem::SeekWaveformItem(QQuickItem *parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents, true);
    setAntialiasing(true);
}

void SeekWaveformItem::setValues(const QVariantList &values)
{
    if (m_values == values)
        return;
    m_values = values;
    ++m_valuesRevision;
    update();
}

void SeekWaveformItem::setPlayedProgress(qreal progress)
{
    progress = qBound<qreal>(0.0, progress, 1.0);
    if (qFuzzyCompare(m_playedProgress, progress))
        return;
    m_playedProgress = progress;
    update();
}

void SeekWaveformItem::setCacheProgress(qreal progress)
{
    progress = qBound<qreal>(0.0, progress, 1.0);
    if (qFuzzyCompare(m_cacheProgress, progress))
        return;
    m_cacheProgress = progress;
    update();
}

void SeekWaveformItem::setBaseColor(const QColor &color)
{
    if (m_baseColor == color)
        return;
    m_baseColor = color;
    update();
}

void SeekWaveformItem::setCacheColor(const QColor &color)
{
    if (m_cacheColor == color)
        return;
    m_cacheColor = color;
    update();
}

void SeekWaveformItem::setPlayedColor(const QColor &color)
{
    if (m_playedColor == color)
        return;
    m_playedColor = color;
    update();
}

QSGNode *SeekWaveformItem::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *)
{
    if (m_values.isEmpty() || width() <= 0.0 || height() <= 0.0) {
        delete oldNode;
        return nullptr;
    }

    auto *root = static_cast<WaveformRootNode *>(oldNode);
    if (!root)
        root = new WaveformRootNode;

    const QRectF fullRect(0.0, 0.0, width(), height());
    root->baseClip.setClipRect(fullRect);
    root->cacheClip.setClipRect(QRectF(0.0, 0.0, width() * m_cacheProgress, height()));
    root->playedClip.setClipRect(QRectF(0.0, 0.0, width() * m_playedProgress, height()));

    if (root->geometryRevision != m_valuesRevision || root->geometrySize != size()) {
        const int columns = qBound(16, static_cast<int>(std::ceil(width() / 2.0)), 512);
        const auto values = renderValues(m_values, columns);
        setLayerGeometry(root->baseLayer, values, width(), height());
        setLayerGeometry(root->cacheLayer, values, width(), height());
        setLayerGeometry(root->playedLayer, values, width(), height());
        root->geometryRevision = m_valuesRevision;
        root->geometrySize = size();
    }

    setLayerColor(root->baseLayer, m_baseColor);
    setLayerColor(root->cacheLayer, m_cacheColor);
    setLayerColor(root->playedLayer, m_playedColor);
    return root;
}

extern "C" void qbz_seek_waveform_register_qml_type()
{
    static std::atomic_bool registered{ false };
    if (registered.exchange(true))
        return;
    qmlRegisterType<SeekWaveformItem>("com.blitzfc.qbz", 1, 0, "SeekWaveformItem");
}
