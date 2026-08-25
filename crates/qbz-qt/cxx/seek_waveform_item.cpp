#include "seek_waveform_item.h"

#include <algorithm>
#include <atomic>
#include <cmath>
#include <limits>
#include <vector>

#include <QtQml/qqml.h>
#include <QtQuick/QSGClipNode>
#include <QtQuick/QSGGeometryNode>
#include <QtQuick/QSGVertexColorMaterial>

namespace {

QSGGeometryNode *makeLayer()
{
    auto *node = new QSGGeometryNode;
    auto *geometry = new QSGGeometry(QSGGeometry::defaultAttributes_ColoredPoint2D(), 0);
    geometry->setDrawingMode(QSGGeometry::DrawTriangles);
    node->setGeometry(geometry);
    node->setFlag(QSGNode::OwnsGeometry);
    node->setMaterial(new QSGVertexColorMaterial);
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
        baseLayer = makeLayer();
        cacheLayer = makeLayer();
        playedLayer = makeLayer();
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
    QColor baseGeometryColor;
    QColor cacheGeometryColor;
    QColor playedGeometryColor;
};

void setVertex(QSGGeometry::ColoredPoint2D &vertex,
               float x,
               float y,
               const QColor &color,
               float alpha)
{
    const float combined = std::clamp(static_cast<float>(color.alphaF()) * alpha, 0.0f, 1.0f);
    vertex.set(x,
               y,
               static_cast<unsigned char>(std::round(color.red() * combined)),
               static_cast<unsigned char>(std::round(color.green() * combined)),
               static_cast<unsigned char>(std::round(color.blue() * combined)),
               static_cast<unsigned char>(std::round(255.0f * combined)));
}

void setQuad(QSGGeometry::ColoredPoint2D *vertices,
             float left,
             float top,
             float right,
             float bottom,
             const QColor &color,
             float topAlpha,
             float bottomAlpha)
{
    setVertex(vertices[0], left, top, color, topAlpha);
    setVertex(vertices[1], left, bottom, color, bottomAlpha);
    setVertex(vertices[2], right, top, color, topAlpha);
    setVertex(vertices[3], right, top, color, topAlpha);
    setVertex(vertices[4], left, bottom, color, bottomAlpha);
    setVertex(vertices[5], right, bottom, color, bottomAlpha);
}

void setLayerGeometry(QSGGeometryNode *node,
                      const std::vector<float> &values,
                      qreal width,
                      qreal height,
                      const QColor &color)
{
    auto *geometry = node->geometry();
    const int count = static_cast<int>(values.size());
    const int visibleBars = static_cast<int>(std::count_if(values.begin(), values.end(),
        [](float value) { return value > 0.0005f; }));
    geometry->allocate(6 + visibleBars * 12);
    geometry->setDrawingMode(QSGGeometry::DrawTriangles);
    auto *vertices = geometry->vertexDataAsColoredPoint2D();
    const float centre = static_cast<float>(height * 0.5);
    const float halfHeight = std::max(1.0f, static_cast<float>(height * 0.47));

    // A permanent hairline makes the temporal axis readable before the whole
    // track has been analysed. Its three clipped color layers retain the
    // existing played/cache/base semantics.
    const float railHalf = std::min(0.55f, halfHeight * 0.16f);
    setQuad(vertices, 0.0f, centre - railHalf, static_cast<float>(width), centre + railHalf,
            color, 0.46f, 0.46f);

    if (count == 0) {
        node->markDirty(QSGNode::DirtyGeometry);
        return;
    }

    constexpr int groupSize = 8;
    const int groupBreaks = (count - 1) / groupSize;
    const float nominalSlot = static_cast<float>(width) / count;
    const float groupGap = std::min(1.7f, std::max(0.65f, nominalSlot * 0.48f));
    const float usableWidth = std::max(1.0f, static_cast<float>(width) - groupBreaks * groupGap);
    const float slot = usableWidth / count;
    const float barWidth = std::max(0.65f, std::min(slot * 0.66f, slot - 0.18f));
    int vertex = 6;

    for (int index = 0; index < count; ++index) {
        const float value = std::clamp(values[index], 0.0f, 1.0f);
        if (value <= 0.0005f)
            continue;
        const float amplitude = std::min(halfHeight,
            0.85f + value * std::max(0.0f, halfHeight - 0.85f));
        const float left = slot * index + groupGap * (index / groupSize)
            + (slot - barWidth) * 0.5f;
        const float right = left + barWidth;
        setQuad(vertices + vertex, left, centre - amplitude, right, centre,
                color, 0.48f, 0.98f);
        vertex += 6;
        setQuad(vertices + vertex, left, centre, right, centre + amplitude,
                color, 0.98f, 0.42f);
        vertex += 6;
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
        values[column] = std::pow(peak, 1.42f);
    }

    std::vector<float> scratch(columns, 0.0f);
    for (int pass = 0; pass < 2; ++pass) {
        scratch.front() = values.front();
        scratch.back() = values.back();
        for (int index = 1; index + 1 < columns; ++index) {
            scratch[index] = values[index - 1] * 0.20f
                + values[index] * 0.60f
                + values[index + 1] * 0.20f;
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

    const bool colorsChanged = root->baseGeometryColor != m_baseColor
        || root->cacheGeometryColor != m_cacheColor
        || root->playedGeometryColor != m_playedColor;
    if (root->geometryRevision != m_valuesRevision || root->geometrySize != size()
        || colorsChanged) {
        const int columns = qBound(24, static_cast<int>(std::ceil(width() / 2.45)), 512);
        const auto values = renderValues(m_values, columns);
        setLayerGeometry(root->baseLayer, values, width(), height(), m_baseColor);
        setLayerGeometry(root->cacheLayer, values, width(), height(), m_cacheColor);
        setLayerGeometry(root->playedLayer, values, width(), height(), m_playedColor);
        root->geometryRevision = m_valuesRevision;
        root->geometrySize = size();
        root->baseGeometryColor = m_baseColor;
        root->cacheGeometryColor = m_cacheColor;
        root->playedGeometryColor = m_playedColor;
    }
    return root;
}

extern "C" void qbz_seek_waveform_register_qml_type()
{
    static std::atomic_bool registered{ false };
    if (registered.exchange(true))
        return;
    qmlRegisterType<SeekWaveformItem>("com.blitzfc.qbz", 1, 0, "SeekWaveformItem");
}
