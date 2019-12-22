PAGES    := guide.html index.html build.html
TEMPLATE := template.html

build: $(PAGES)

%.html: %.md $(TEMPLATE)
	mkdir -p html
	cmark --unsafe -t html $< | ./build.sh $* $(TEMPLATE) > html/$@

.PHONY: build
