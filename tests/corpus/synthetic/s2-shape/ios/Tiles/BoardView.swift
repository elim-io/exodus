import SwiftUI

struct BoardView: View {
    @State private var seconds = 0
    let timer = Timer.publish(every: 1, on: .main, in: .common).autoconnect()

    var body: some View {
        Text("\(seconds) s")
            .onReceive(timer) { _ in seconds += 1 }
    }

    func save(_ score: Int) throws {
        let url = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("score.json")
        try JSONEncoder().encode(["score": score]).write(to: url)
    }
}
