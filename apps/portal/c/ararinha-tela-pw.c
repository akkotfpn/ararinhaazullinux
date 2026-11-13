// SPDX-License-Identifier: GPL-3.0-or-later
//
// ararinha-tela-pw: entrega a tela da Plumagem ao PipeWire, para o portal
// de compartilhamento de tela (OBS, Discord, Chromium, Firefox).
//
// Como funciona:
//   1. Cria $XDG_RUNTIME_DIR/ararinha-tela.ativa com o próprio PID. A
//      Plumagem vê o arquivo e passa a gravar cada quadro (até 30 por
//      segundo) em $XDG_RUNTIME_DIR/ararinha-tela.quadro: 64 bytes de
//      cabeçalho (u32 LE: "ARAT", número do quadro, largura, altura, passo)
//      e os pixels RGBA.
//   2. Cria um nó de vídeo "Video/Source" no PipeWire e manda o quadro mais
//      novo 30 vezes por segundo.
//   3. Escreve "<id do nó> <largura> <altura>" na saída padrão (o portal lê
//      e entrega ao app) e roda até a entrada padrão fechar ou receber
//      SIGTERM. Ao sair, apaga os dois arquivos.

#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#include <pipewire/pipewire.h>
#include <spa/param/buffers.h>
#include <spa/param/video/format-utils.h>
#include <spa/pod/builder.h>

#define MAGICO 0x54415241u /* "ARAT" em little-endian */

struct dados {
	struct pw_main_loop *laco;
	struct pw_stream *fluxo;
	struct spa_source *relogio;
	struct spa_video_info_raw formato;
	int fd;
	uint8_t *mapa;
	size_t tam;
	char quadro[512];
	char ativa[512];
	int avisado;
	uint32_t w, h;
};

static uint32_t le32(const uint8_t *p)
{
	uint32_t v;
	memcpy(&v, p, 4);
	return v;
}

/* Abre (ou remapeia, se o tamanho mudou) o arquivo do quadro. */
static void mapear(struct dados *d)
{
	struct stat st;
	if (d->fd < 0) {
		d->fd = open(d->quadro, O_RDONLY | O_CLOEXEC);
		if (d->fd < 0)
			return;
	}
	if (fstat(d->fd, &st) < 0)
		return;
	if ((size_t)st.st_size == d->tam && d->mapa != NULL)
		return;
	if (d->mapa != NULL)
		munmap(d->mapa, d->tam);
	d->mapa = NULL;
	d->tam = 0;
	if (st.st_size < 64)
		return;
	void *m = mmap(NULL, (size_t)st.st_size, PROT_READ, MAP_SHARED, d->fd, 0);
	if (m == MAP_FAILED)
		return;
	d->mapa = m;
	d->tam = (size_t)st.st_size;
}

static void ao_processar(void *u)
{
	struct dados *d = u;
	struct pw_buffer *b = pw_stream_dequeue_buffer(d->fluxo);
	if (b == NULL)
		return;
	struct spa_buffer *buf = b->buffer;
	uint8_t *dst = buf->datas[0].data;
	if (dst == NULL) {
		pw_stream_queue_buffer(d->fluxo, b);
		return;
	}
	mapear(d);
	uint32_t fw = 0, fh = 0, fpasso = 0;
	if (d->mapa != NULL && le32(d->mapa) == MAGICO) {
		fw = le32(d->mapa + 8);
		fh = le32(d->mapa + 12);
		fpasso = le32(d->mapa + 16);
	}
	uint32_t w = d->formato.size.width, h = d->formato.size.height;
	uint32_t passo = SPA_ROUND_UP_N(w * 4, 4);
	int bgr = d->formato.format == SPA_VIDEO_FORMAT_BGRx || d->formato.format == SPA_VIDEO_FORMAT_BGRA;
	uint32_t maximo = buf->datas[0].maxsize;
	for (uint32_t y = 0; y < h; y++) {
		if ((size_t)(y + 1) * passo > maximo)
			break;
		uint8_t *linha = dst + (size_t)y * passo;
		if (y < fh && d->mapa != NULL && 64 + (size_t)(y + 1) * fpasso <= d->tam) {
			const uint8_t *src = d->mapa + 64 + (size_t)y * fpasso;
			uint32_t n = fw < w ? fw : w;
			if (bgr) {
				for (uint32_t x = 0; x < n; x++) {
					linha[4 * x] = src[4 * x + 2];
					linha[4 * x + 1] = src[4 * x + 1];
					linha[4 * x + 2] = src[4 * x];
					linha[4 * x + 3] = 255;
				}
			} else {
				memcpy(linha, src, (size_t)n * 4);
			}
			if (n < w)
				memset(linha + (size_t)n * 4, 0, (size_t)(w - n) * 4);
		} else {
			memset(linha, 0, passo);
		}
	}
	buf->datas[0].chunk->offset = 0;
	buf->datas[0].chunk->size = passo * h;
	buf->datas[0].chunk->stride = (int32_t)passo;
	pw_stream_queue_buffer(d->fluxo, b);
}

static void ao_mudar_param(void *u, uint32_t id, const struct spa_pod *param)
{
	struct dados *d = u;
	if (param == NULL || id != SPA_PARAM_Format)
		return;
	if (spa_format_video_raw_parse(param, &d->formato) < 0)
		return;
	uint32_t passo = SPA_ROUND_UP_N(d->formato.size.width * 4, 4);
	uint8_t mem[1024];
	struct spa_pod_builder b = SPA_POD_BUILDER_INIT(mem, sizeof(mem));
	const struct spa_pod *params[1];
	params[0] = spa_pod_builder_add_object(&b,
		SPA_TYPE_OBJECT_ParamBuffers, SPA_PARAM_Buffers,
		SPA_PARAM_BUFFERS_buffers, SPA_POD_CHOICE_RANGE_Int(4, 2, 8),
		SPA_PARAM_BUFFERS_blocks, SPA_POD_Int(1),
		SPA_PARAM_BUFFERS_size, SPA_POD_Int((int32_t)(passo * d->formato.size.height)),
		SPA_PARAM_BUFFERS_stride, SPA_POD_Int((int32_t)passo),
		SPA_PARAM_BUFFERS_dataType, SPA_POD_CHOICE_FLAGS_Int((1 << SPA_DATA_MemFd) | (1 << SPA_DATA_MemPtr)));
	pw_stream_update_params(d->fluxo, params, 1);
}

static void ao_mudar_estado(void *u, enum pw_stream_state antes, enum pw_stream_state agora, const char *erro)
{
	struct dados *d = u;
	(void)antes;
	if (!d->avisado) {
		uint32_t no = pw_stream_get_node_id(d->fluxo);
		if (no != SPA_ID_INVALID) {
			printf("%u %u %u\n", no, d->w, d->h);
			fflush(stdout);
			d->avisado = 1;
		}
	}
	if (agora == PW_STREAM_STATE_ERROR) {
		fprintf(stderr, "ararinha-tela-pw: %s\n", erro ? erro : "erro no fluxo");
		pw_main_loop_quit(d->laco);
	}
}

static const struct pw_stream_events eventos = {
	PW_VERSION_STREAM_EVENTS,
	.state_changed = ao_mudar_estado,
	.param_changed = ao_mudar_param,
	.process = ao_processar,
};

static void ao_relogio(void *u, uint64_t vezes)
{
	struct dados *d = u;
	(void)vezes;
	if (pw_stream_get_state(d->fluxo, NULL) == PW_STREAM_STATE_STREAMING)
		pw_stream_trigger_process(d->fluxo);
}

static void ao_stdin(void *u, int fd, uint32_t mascara)
{
	struct dados *d = u;
	char lixo[64];
	ssize_t n = read(fd, lixo, sizeof(lixo));
	if (n <= 0 || (mascara & (SPA_IO_HUP | SPA_IO_ERR)))
		pw_main_loop_quit(d->laco);
}

static void ao_sinal(void *u, int sinal)
{
	struct dados *d = u;
	(void)sinal;
	pw_main_loop_quit(d->laco);
}

int main(int argc, char *argv[])
{
	struct dados d;
	memset(&d, 0, sizeof(d));
	d.fd = -1;
	const char *rt = getenv("XDG_RUNTIME_DIR");
	if (rt == NULL) {
		fprintf(stderr, "ararinha-tela-pw: sem XDG_RUNTIME_DIR\n");
		return 1;
	}
	snprintf(d.quadro, sizeof(d.quadro), "%s/ararinha-tela.quadro", rt);
	snprintf(d.ativa, sizeof(d.ativa), "%s/ararinha-tela.ativa", rt);

	/* Avisa a Plumagem e espera o primeiro quadro (até 6 s). */
	FILE *f = fopen(d.ativa, "w");
	if (f == NULL) {
		fprintf(stderr, "ararinha-tela-pw: %s: %s\n", d.ativa, strerror(errno));
		return 1;
	}
	fprintf(f, "%d\n", (int)getpid());
	fclose(f);
	for (int i = 0; i < 120; i++) {
		mapear(&d);
		if (d.mapa != NULL && le32(d.mapa) == MAGICO && le32(d.mapa + 4) != 0) {
			d.w = le32(d.mapa + 8);
			d.h = le32(d.mapa + 12);
			if (d.w > 0 && d.h > 0)
				break;
		}
		struct timespec t = { 0, 50 * 1000 * 1000 };
		nanosleep(&t, NULL);
	}
	if (d.w == 0 || d.h == 0) {
		fprintf(stderr, "ararinha-tela-pw: a Plumagem não mandou a tela\n");
		unlink(d.ativa);
		return 1;
	}

	pw_init(&argc, &argv);
	d.laco = pw_main_loop_new(NULL);
	struct pw_loop *laco = pw_main_loop_get_loop(d.laco);
	pw_loop_add_signal(laco, SIGINT, ao_sinal, &d);
	pw_loop_add_signal(laco, SIGTERM, ao_sinal, &d);
	pw_loop_add_io(laco, 0, SPA_IO_IN | SPA_IO_HUP | SPA_IO_ERR, false, ao_stdin, &d);

	d.fluxo = pw_stream_new_simple(laco, "ararinha-tela",
		pw_properties_new(
			PW_KEY_MEDIA_CLASS, "Video/Source",
			PW_KEY_MEDIA_TYPE, "Video",
			PW_KEY_MEDIA_CATEGORY, "Capture",
			PW_KEY_MEDIA_ROLE, "Screen",
			PW_KEY_NODE_DESCRIPTION, "Tela do Ararinha",
			NULL),
		&eventos, &d);

	uint8_t mem[1024];
	struct spa_pod_builder b = SPA_POD_BUILDER_INIT(mem, sizeof(mem));
	const struct spa_pod *params[1];
	struct spa_rectangle tamanho = SPA_RECTANGLE(d.w, d.h);
	struct spa_fraction zero = SPA_FRACTION(0, 1);
	struct spa_fraction trinta = SPA_FRACTION(30, 1);
	struct spa_fraction um = SPA_FRACTION(1, 1);
	params[0] = spa_pod_builder_add_object(&b,
		SPA_TYPE_OBJECT_Format, SPA_PARAM_EnumFormat,
		SPA_FORMAT_mediaType, SPA_POD_Id(SPA_MEDIA_TYPE_video),
		SPA_FORMAT_mediaSubtype, SPA_POD_Id(SPA_MEDIA_SUBTYPE_raw),
		SPA_FORMAT_VIDEO_format, SPA_POD_CHOICE_ENUM_Id(5,
			SPA_VIDEO_FORMAT_BGRx, SPA_VIDEO_FORMAT_BGRx, SPA_VIDEO_FORMAT_RGBx,
			SPA_VIDEO_FORMAT_BGRA, SPA_VIDEO_FORMAT_RGBA),
		SPA_FORMAT_VIDEO_size, SPA_POD_Rectangle(&tamanho),
		SPA_FORMAT_VIDEO_framerate, SPA_POD_Fraction(&zero),
		SPA_FORMAT_VIDEO_maxFramerate, SPA_POD_CHOICE_RANGE_Fraction(&trinta, &um, &trinta));

	if (pw_stream_connect(d.fluxo, PW_DIRECTION_OUTPUT, PW_ID_ANY,
			PW_STREAM_FLAG_DRIVER | PW_STREAM_FLAG_MAP_BUFFERS, params, 1) < 0) {
		fprintf(stderr, "ararinha-tela-pw: não deu para conectar ao PipeWire\n");
		unlink(d.ativa);
		return 1;
	}

	d.relogio = pw_loop_add_timer(laco, ao_relogio, &d);
	struct timespec valor = { 0, 33 * 1000 * 1000 };
	struct timespec intervalo = { 0, 33 * 1000 * 1000 };
	pw_loop_update_timer(laco, d.relogio, &valor, &intervalo, false);

	pw_main_loop_run(d.laco);

	pw_stream_destroy(d.fluxo);
	pw_main_loop_destroy(d.laco);
	pw_deinit();
	unlink(d.ativa);
	unlink(d.quadro);
	return 0;
}
