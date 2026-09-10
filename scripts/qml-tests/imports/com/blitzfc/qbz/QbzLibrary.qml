pragma Singleton
import QtQuick
QtObject { signal libraryArtworkReady(string key, string path); signal pinChanged(string key, bool value); signal libraryFavoriteChanged(string key, bool value) }
