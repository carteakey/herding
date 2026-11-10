import Foundation
@testable import SheprdRemoteCore
import Testing

/// What a terminal tab's menu may move into a chat
/// (`POST /api/v2/term/{pane}/convert`).
struct ConvertOfferTests {
    @Test func readsThePaneTheComputerOffers() throws {
        let message = try JSONSerialization.jsonObject(with: WireTests.fixture("home")) as? [String: Any]
        let data = try JSONSerialization.data(withJSONObject: message?["home"] ?? [:])
        let home = try JSONDecoder().decode(Home.self, from: data)
        #expect(home.workspaces[0].terminals[0].convert == .init(agent: "claude", pane: "p2"))
    }

    @Test func anOlderComputerOffersNothing() throws {
        let json = Data(#"{"tab":"t1","label":"zsh","cwd":null,"zoomed":false,"panes":[]}"#.utf8)
        let row = try JSONDecoder().decode(Home.TerminalRow.self, from: json)
        #expect(row.convert == nil)
    }
}
