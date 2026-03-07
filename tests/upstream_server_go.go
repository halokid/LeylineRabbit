package main

import (
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
)

func main() {
	r := gin.Default()

	r.GET("/ping", func(c *gin.Context) {
		c.JSON(200, gin.H{
			"message":  "pong",
			"upstream": "go-gin-server",
			"status":   "healthy",
			"port":     8083,
		})
	})

	r.POST("/echo", func(c *gin.Context) {
		var json map[string]interface{}
		if err := c.ShouldBindJSON(&json); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}
		c.JSON(200, gin.H{
			"received": json,
			"upstream": "go-gin-server",
			"status":   "healthy",
			"port":     8083,
		})
	})

	r.GET("/timeout", func(c *gin.Context) {
		time.Sleep(20 * time.Second) // Sleep longer than gateway timeout
		c.JSON(200, gin.H{
			"message": "This should timeout",
		})
	})

	r.Run(":8083") // listen and serve on 0.0.0.0:8083
}