docs-serve:
	mkdocs serve -f web/docs/mkdocs.yml

docs-build:
	mkdocs build -f web/docs/mkdocs.yml

docs-build-strict:
	mkdocs build --strict -f web/docs/mkdocs.yml

docs-build-site:
	bash infra/scripts/build_docs_site.sh

docs-docker-build:
	docker build -f infra/docker/Dockerfile.docs -t palmscript-docs .

docs-docker-run:
	docker run --rm -p 8080:8080 palmscript-docs

ide-docker-build:
	docker build -f infra/docker/Dockerfile.ide -t palmscript-ide .

ide-docker-run:
	docker run --rm -p 8080:8080 palmscript-ide
