package main

import (
	"database/sql"
	"encoding/json"
	"log"
	"net/http"
	"os"
	"time"

	_ "modernc.org/sqlite"
)

func main() {
	db, err := sql.Open("sqlite", os.Getenv("DB_PATH"))
	if err != nil {
		log.Fatal(err)
	}

	go refresh(db)

	http.HandleFunc("/card", func(w http.ResponseWriter, r *http.Request) {
		var title, image string
		err := db.QueryRow("select title, image from cards where url = ?", r.URL.Query().Get("url")).Scan(&title, &image)
		if err != nil {
			http.NotFound(w, r)
			return
		}
		json.NewEncoder(w).Encode(map[string]string{"title": title, "image": image})
	})
	log.Fatal(http.ListenAndServe(":8080", nil))
}

// refresh marks cards older than a day as stale, every 10 minutes.
func refresh(db *sql.DB) {
	ticker := time.NewTicker(10 * time.Minute)
	for range ticker.C {
		if _, err := db.Exec("update cards set stale = 1 where fetched_at < datetime('now', '-1 day')"); err != nil {
			log.Print(err)
		}
	}
}
