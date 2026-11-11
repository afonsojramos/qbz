import QtQuick
import QtTest
import com.blitzfc.qbz
import "../../crates/qbz-qt/qml/settings" as Settings
Item {
    width: 1000; height: 1000
    Settings.MediaServerSettings {
        id: jellyfin; width: 500
        server: "jellyfin"; title: "Jellyfin"; subtitle: ""
        urlPlaceholder: ""; testHint: ""; credentialNote: ""; syncCost: ""
        state: ({enabled:true,hasCredential:true,serverUrl:"http://localhost",username:"user"})
    }
    Settings.MediaServerSettings {
        id: subsonic; x: 500; width: 500
        server: "subsonic"; title: "Subsonic"; subtitle: ""
        urlPlaceholder: ""; testHint: ""; credentialNote: ""; syncCost: ""
        state: ({enabled:true,hasCredential:true,serverUrl:"http://localhost",username:"user"})
    }
    TestCase {
        name: "MediaConnectionStatus"; when: windowShown
        function init() { QbzLocal.mediaStatus="{}" }
        function test_pairing_only_jellyfin_and_no_password_required() {
            verify(findChild(jellyfin, "quickConnectRow").visible)
            verify(!findChild(subsonic, "quickConnectRow").visible)
            verify(!jellyfin.canConnect)
            verify(findChild(jellyfin, "quickConnectButton").btnEnabled)
            QbzLocal.mediaStatus=JSON.stringify({jellyfin:{busy:true,phase:"pairing-waiting",pairing_code:"123456"}})
            verify(jellyfin.pairing)
            verify(!subsonic.busy)
            compare(findChild(jellyfin, "quickConnectCode").text,"123456")
            compare(jellyfin.statusText,"Waiting for authorization…")
            verify(findChild(jellyfin, "quickConnectButton").btnEnabled)
            jellyfin.visible=false
            verify(QbzLocal.pairingCancels > 0)
            jellyfin.visible=true
        }
        function test_saved_does_not_claim_verified() {
            compare(jellyfin.statusText,"Saved connection. Sync to check access.")
        }
        function test_busy_is_per_provider() {
            QbzLocal.mediaStatus=JSON.stringify({jellyfin:{busy:true,phase:"authenticating"},subsonic:{busy:false}})
            verify(jellyfin.busy); verify(!subsonic.busy)
            compare(jellyfin.statusText,"Signing in…")
        }
        function test_sync_does_not_claim_other_provider_is_syncing() {
            QbzLocal.mediaStatus=JSON.stringify({jellyfin:{syncing:true,progress:"50/100"},subsonic:{phase:"failed",error:"sign-in refused"}})
            verify(jellyfin.busy); verify(!subsonic.busy)
            compare(jellyfin.statusText,"Connected. Syncing library…")
            compare(subsonic.statusText,"sign-in refused")
        }
        function test_both_can_progress_independently() {
            QbzLocal.mediaStatus=JSON.stringify({jellyfin:{syncing:true,progress:"50/100"},subsonic:{syncing:true,progress:"20/80"}})
            compare(jellyfin.operation.progress,"50/100")
            compare(subsonic.operation.progress,"20/80")
        }
        function test_test_connection_is_not_login() {
            QbzLocal.mediaStatus=JSON.stringify({subsonic:{phase:"reachable"}})
            compare(subsonic.statusText,"Server reachable. Credentials have not been checked.")
        }
    }
}
