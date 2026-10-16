// Original prototype implementation; MIT, see LICENSE.
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>
#include <algorithm>
#include <array>
#include <cmath>
#include <tuple>
#include <vector>

namespace {
struct Record {
    QString id;
    bool snippet;
    double created;
    std::array<QString, 4> fields;
    double count = 0, last = 0;
};
struct Engine {
    std::vector<Record> records;
    QByteArray output;
};
using TextScore = std::tuple<int, int, int, int>;
using Score = std::tuple<TextScore, double, double, QString>;
struct Hit { Score score; const Record* record; };

std::pair<int, int> match(const QString& query, const QString& field) {
    if (query == field) return {0, 0};
    if (field.startsWith(query)) return {1, int(field.size() - query.size())};
    auto offset = field.indexOf(query);
    if (offset >= 0) return {2, int(offset)};
    qsizetype position = -1, first = -1;
    for (auto character : query) {
        position = field.indexOf(character, position + 1);
        if (position < 0) return {-1, -1};
        if (first < 0) first = position;
    }
    return {3, int(position - first - query.size() + 1)};
}
}

extern "C" {
void* snipboard_create(const char* input) {
    auto* engine = new Engine;
    const auto document = QJsonDocument::fromJson(input);
    for (const auto item : document.array()) {
        const auto object = item.toObject();
        Record record;
        record.id = object["id"].toString();
        record.snippet = object["kind"].toString() == "snippet";
        record.created = object["created"].toDouble();
        int i = 0;
        for (auto key : {"keyword", "name", "notes", "body"})
            record.fields[i++] = object[key].toString();
        engine->records.push_back(std::move(record));
    }
    return engine;
}

void snipboard_destroy(void* pointer) { delete static_cast<Engine*>(pointer); }

void snipboard_usage(void* pointer, const char* input) {
    const auto usage = QJsonDocument::fromJson(input).object();
    for (auto& record : static_cast<Engine*>(pointer)->records) {
        const auto stat = usage[record.id].toObject();
        record.count = stat["count"].toDouble();
        record.last = stat["last"].toDouble();
    }
}

const char* snipboard_search(void* pointer, const char* input) {
    auto& engine = *static_cast<Engine*>(pointer);
    const auto request = QJsonDocument::fromJson(input).object();
    const QString query = request["query"].toString();
    const QString kind = request["kind"].toString();
    const double now = request["now"].toDouble();
    const int limit = request["limit"].toInt(60);
    std::vector<Hit> hits;
    hits.reserve(engine.records.size());
    for (const auto& record : engine.records) {
        if (kind == "snippet" && !record.snippet) continue;
        if (kind == "clipboard" && record.snippet) continue;
        TextScore best{9, 9, 9, 9};
        if (query.isEmpty()) best = {record.snippet ? 1 : 0, 0, 0, 0};
        else {
            for (int i = 0; i < 4; ++i) {
                auto [tier, spread] = match(query, record.fields[i]);
                if (tier >= 0) best = std::min(best, TextScore{i < 2 ? 0 : 1, tier, i, spread});
            }
            if (std::get<0>(best) == 9) continue;
        }
        const double age = std::max(0.0, now - record.last) / 86400.0;
        const double usage = record.snippet ? std::log1p(record.count) * std::exp(-age / 14.0) : 0;
        hits.push_back({{best, -usage, record.snippet ? 0 : -record.created, record.id}, &record});
    }
    auto less = [](const Hit& a, const Hit& b) { return a.score < b.score; };
    QJsonArray result;
    auto append = [&](std::vector<Hit>& group, int maximum) {
        const auto size = std::min(group.size(), size_t(std::max(0, maximum)));
        std::partial_sort(group.begin(), group.begin() + size, group.end(), less);
        for (size_t i = 0; i < size; ++i) result.append(group[i].record->id);
    };
    if (query.isEmpty() && kind == "all") {
        std::vector<Hit> clips, snippets;
        for (const auto& hit : hits) (hit.record->snippet ? snippets : clips).push_back(hit);
        append(clips, limit / 2);
        append(snippets, limit / 2);
    } else append(hits, limit);
    engine.output = QJsonDocument(result).toJson(QJsonDocument::Compact);
    return engine.output.constData();
}
}
